use std::io::{Read, Write};

use aes::Aes256;
use cbc::{Decryptor, Encryptor};
use cipher::{
    block_padding::NoPadding, block_padding::Pkcs7, BlockModeDecrypt, BlockModeEncrypt, KeyIvInit,
};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use getrandom::getrandom;
use hmac::{digest::KeyInit as HmacKeyInit, Hmac, Mac};
use sha2::Sha256;

use crate::save_parser::error::SaveError;

type Aes256CbcDec = Decryptor<Aes256>;
type Aes256CbcEnc = Encryptor<Aes256>;
type HmacSha256 = Hmac<Sha256>;

const AES_KEY: [u8; 32] = [
    0x2a, 0x5f, 0xcb, 0x17, 0x91, 0xd2, 0x2f, 0xb6, 0x02, 0x45, 0xb3, 0xd8, 0x36, 0x9e, 0xd0, 0xb2,
    0xc2, 0x73, 0x71, 0x56, 0x3f, 0xbf, 0x1f, 0x3c, 0x9e, 0xdf, 0x6b, 0x11, 0x82, 0x5a, 0x5d, 0x0a,
];

const SCSC_HEADER_SIZE: usize = 56;

/// The on-disk format of a save/config file, detected from its magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFormat {
    /// `ScsC`: AES-256-CBC + zlib. Standard ETS2/ATS save format.
    EncryptedScsC,
    /// `SiiN`: plaintext SII (`g_save_format 2`). Fully supported.
    PlaintextSiiN,
    /// `BSII`: Steam-Cloud binary format. Cannot be edited as text.
    BinaryBsii,
    /// Too small to hold a magic, or an unrecognized magic.
    Unknown,
}

/// Detect the file format from its magic bytes. Never fails;
/// files too small for a magic report [`SaveFormat::Unknown`].
pub fn detect_format(data: &[u8]) -> SaveFormat {
    if data.len() < 4 {
        return SaveFormat::Unknown;
    }
    match &data[..4] {
        b"ScsC" => SaveFormat::EncryptedScsC,
        b"SiiN" => SaveFormat::PlaintextSiiN,
        b"BSII" => SaveFormat::BinaryBsii,
        _ => SaveFormat::Unknown,
    }
}

pub fn decompress_save(data: &[u8]) -> Result<String, SaveError> {
    match detect_format(data) {
        SaveFormat::EncryptedScsC => decrypt_scsc(data),
        SaveFormat::BinaryBsii => Err(SaveError::Compression(
            "Binary format BSII not supported. Use g_save_format 2.".into(),
        )),
        SaveFormat::PlaintextSiiN => Ok(std::str::from_utf8(data)?.to_string()),
        SaveFormat::Unknown => {
            if data.len() < 4 {
                return Err(SaveError::Compression(format!(
                    "File too small ({} bytes) to detect a save format.",
                    data.len()
                )));
            }
            std::str::from_utf8(data)
                .map(|s| s.to_string())
                .map_err(|_| {
                    SaveError::UnknownFormat(format!("Unknown format, magic: {:?}", &data[..4]))
                })
        }
    }
}

fn decrypt_scsc(data: &[u8]) -> Result<String, SaveError> {
    if data.len() < SCSC_HEADER_SIZE {
        return Err(SaveError::Compression(format!(
            "ScsC file too short ({} bytes, need at least {SCSC_HEADER_SIZE}). \
             The save is truncated or corrupted.",
            data.len()
        )));
    }

    let iv: &[u8; 16] = data[36..52]
        .try_into()
        .map_err(|_| SaveError::Compression("Invalid IV size".into()))?;

    let _decompressed_size = u32::from_le_bytes(
        data[52..56]
            .try_into()
            .map_err(|_| SaveError::Compression("Invalid decompressed size".into()))?,
    ) as usize;

    let encrypted = &data[56..];

    // The ciphertext must fill whole AES blocks. A truncated file must be
    // reported as corrupt instead of being silently zero-padded.
    if encrypted.is_empty() || !encrypted.len().is_multiple_of(16) {
        return Err(SaveError::Compression(format!(
            "ScsC payload has invalid length ({} bytes, must be a non-empty \
             multiple of 16). The save is truncated or corrupted.",
            encrypted.len()
        )));
    }

    // Decrypt AES-CBC
    let mut buf = encrypted.to_vec();

    let decrypted_len = {
        let decrypted = Aes256CbcDec::new_from_slices(&AES_KEY, iv)
            .map_err(|e| SaveError::Crypto(format!("AES init failed: {e}")))?
            .decrypt_padded::<NoPadding>(&mut buf)
            .map_err(|e| {
                SaveError::Crypto(format!(
                    "AES decrypt failed (wrong game or corrupted file): {e}"
                ))
            })?;
        decrypted.len()
    };
    buf.truncate(decrypted_len);

    // Zlib decompress
    let mut decoder = ZlibDecoder::new(&buf[..]);
    let mut result = Vec::new();
    decoder.read_to_end(&mut result).map_err(|e| {
        SaveError::Compression(format!(
            "Zlib decompression failed (file is corrupted or uses an \
             unsupported format): {e}"
        ))
    })?;

    if result.len() >= 4 && &result[..4] == b"BSII" {
        return Err(SaveError::Compression(
            "Binary format BSII not supported. Load the save in Euro Truck Simulator 2 and save again to convert, or set g_save_format 2 in config.cfg.".into(),
        ));
    }

    let text = String::from_utf8(result)
        .map_err(|e| SaveError::Compression(format!("Save uses an unsupported format: {e}",)))?;

    Ok(text)
}

pub fn compress_save(content: &str) -> Result<Vec<u8>, SaveError> {
    let data = content.as_bytes();
    let decompressed_size = data.len();

    // Zlib compress
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder
        .write_all(data)
        .map_err(|e| SaveError::Compression(format!("Zlib compression failed: {e}")))?;
    let compressed = encoder
        .finish()
        .map_err(|e| SaveError::Compression(format!("Zlib compression finalize failed: {e}")))?;

    // Generate random IV
    let mut iv = [0u8; 16];
    getrandom(&mut iv).map_err(|e| SaveError::Crypto(format!("Failed to generate IV: {e}")))?;

    // AES-CBC encrypt
    let block_size = 16;
    let padded_len = compressed.len() + (block_size - (compressed.len() % block_size));
    let mut enc_buf = vec![0u8; padded_len];
    enc_buf[..compressed.len()].copy_from_slice(&compressed);

    let encrypted = Aes256CbcEnc::new_from_slices(&AES_KEY, &iv)
        .map_err(|e| SaveError::Crypto(format!("AES init failed: {e}")))?
        .encrypt_padded::<Pkcs7>(&mut enc_buf, compressed.len())
        .map_err(|e| SaveError::Crypto(format!("AES encrypt failed: {e}")))?;
    let encrypted = encrypted.to_vec();

    // HMAC-SHA256
    let mut mac = HmacSha256::new_from_slice(&AES_KEY)
        .map_err(|e| SaveError::Crypto(format!("HMAC init failed: {e}")))?;
    mac.update(&iv);
    mac.update(&encrypted);
    let hmac_result = mac.finalize().into_bytes();

    // Build output
    let mut output = Vec::with_capacity(SCSC_HEADER_SIZE + encrypted.len());
    output.extend_from_slice(b"ScsC");
    output.extend_from_slice(&hmac_result);
    output.extend_from_slice(&iv);
    output.extend_from_slice(&(decompressed_size as u32).to_le_bytes());
    output.extend_from_slice(&encrypted);

    Ok(output)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_decompress_siin() {
        let data = b"SiiNunit\n{\n}\n";
        let result = decompress_save(data).unwrap();
        assert_eq!(result, "SiiNunit\n{\n}\n");
    }

    #[test]
    fn test_decompress_plain_utf8() {
        let data = b"plain text content";
        let result = decompress_save(data).unwrap();
        assert_eq!(result, "plain text content");
    }

    #[test]
    fn test_decompress_bsii_returns_error() {
        let data = b"BSII some binary data";
        let result = decompress_save(data);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SaveError::Compression(_)));
        assert!(err.to_string().contains("g_save_format"));
    }

    #[test]
    fn test_decompress_empty() {
        let result = decompress_save(b"");
        assert!(result.is_err());
    }

    #[test]
    fn test_decompress_too_small() {
        let result = decompress_save(b"abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_compress_decompress_roundtrip() {
        let content = "SiiNunit\n{\n money_account: 100500\n experience_points: 5000\n}\n";
        let compressed = compress_save(content).unwrap();
        let decompressed = decompress_save(&compressed).unwrap();
        assert_eq!(decompressed, content);
    }

    #[test]
    fn test_compress_decompress_empty_string() {
        let compressed = compress_save("").unwrap();
        let decompressed = decompress_save(&compressed).unwrap();
        assert_eq!(decompressed, "");
    }

    #[test]
    fn test_compress_output_has_scsc_header() {
        let content = "test data";
        let compressed = compress_save(content).unwrap();
        assert_eq!(&compressed[..4], b"ScsC");
        assert!(compressed.len() > 56); // header size
    }

    #[test]
    fn test_compress_produces_valid_hmac() {
        // Re-encrypting the same plaintext should produce different output
        // because of random IV (so HMAC will differ)
        let content = "test data";
        let a = compress_save(content).unwrap();
        let b = compress_save(content).unwrap();
        assert_ne!(
            a, b,
            "two compressions of same data should differ due to random IV"
        );
    }

    #[test]
    fn test_decompress_invalid_utf8() {
        let data = b"\xff\xfe\x00\x01";
        let result = decompress_save(data);
        // Should either be an unknown format error or a UTF-8 error
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_format_scsc() {
        assert_eq!(
            detect_format(b"ScsC rest of file"),
            SaveFormat::EncryptedScsC
        );
    }

    #[test]
    fn test_detect_format_siin() {
        assert_eq!(
            detect_format(b"SiiNunit\n{\n}\n"),
            SaveFormat::PlaintextSiiN
        );
    }

    #[test]
    fn test_detect_format_bsii() {
        assert_eq!(detect_format(b"BSII binary"), SaveFormat::BinaryBsii);
    }

    #[test]
    fn test_detect_format_too_small() {
        assert_eq!(detect_format(b""), SaveFormat::Unknown);
        assert_eq!(detect_format(b"abc"), SaveFormat::Unknown);
    }

    #[test]
    fn test_detect_format_unknown_magic() {
        assert_eq!(detect_format(b"XXXX data"), SaveFormat::Unknown);
    }

    #[test]
    fn test_decompress_truncated_scsc_payload_errors() {
        // Valid header but payload length is not a multiple of 16:
        // must be reported, never silently padded.
        let mut data = vec![0u8; SCSC_HEADER_SIZE + 20];
        data[..4].copy_from_slice(b"ScsC");
        let result = decompress_save(&data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid length"));
    }

    #[test]
    fn test_decompress_empty_scsc_payload_errors() {
        let mut data = vec![0u8; SCSC_HEADER_SIZE];
        data[..4].copy_from_slice(b"ScsC");
        let result = decompress_save(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_scsc_roundtrip_detects_scsc() {
        let compressed = compress_save("SiiNunit\n{\n}\n").unwrap();
        assert_eq!(detect_format(&compressed), SaveFormat::EncryptedScsC);
    }
}
