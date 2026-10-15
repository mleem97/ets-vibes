export interface ConfigRange {
  min: number;
  max: number;
  step: number;
}

export const CONFIG_RANGES: Record<string, ConfigRange> = {
  // Documented enum flags (SCS wiki / mapping guide): the slider snaps to
  // exactly the allowed values, mirroring KEY_DOMAINS in the backend.
  g_adviser: { min: 0, max: 2, step: 1 },
  g_brake_light_all: { min: 0, max: 1, step: 1 },
  g_cam_steering_reverse: { min: 0, max: 2, step: 1 },
  g_clock_24: { min: 0, max: 1, step: 1 },
  // g_developer 2 = map editor setup (per SCS mapping guide), so 0..2.
  g_developer: { min: 0, max: 2, step: 1 },
  // g_console is a plain on/off flag.
  g_console: { min: 0, max: 1, step: 1 },
  g_fahrenheit: { min: 0, max: 1, step: 1 },
  g_force_load_selector: { min: -1, max: 1, step: 1 },
  g_gallon: { min: 0, max: 1, step: 1 },
  g_gfx_advanced: { min: 0, max: 1, step: 1 },
  g_global_force_load_selector: { min: 0, max: 1, step: 1 },
  g_grass_density: { min: 0, max: 3, step: 1 },
  g_hshifter_layout: { min: 0, max: 2, step: 1 },
  g_hud_speed_limit: { min: 0, max: 2, step: 1 },
  g_income_factor: { min: 0, max: 1, step: 0.05 },
  g_mph: { min: 0, max: 1, step: 1 },
  g_news: { min: -1, max: 1, step: 1 },
  g_online_loading_screens: { min: -1, max: 1, step: 1 },
  g_parking_difficulty: { min: 0, max: 3, step: 1 },
  g_rain_reflection: { min: 0, max: 3, step: 1 },
  g_reflection: { min: 0, max: 3, step: 1 },
  g_save_format: { min: 0, max: 3, step: 1 },
  g_steam_screenshots: { min: 0, max: 2, step: 1 },
  g_trans: { min: 0, max: 3, step: 1 },
  g_truck_light_quality: { min: 0, max: 1, step: 1 },
  g_veg_detail: { min: 0, max: 2, step: 1 },
  r_color_correction: { min: 0, max: 2, step: 1 },
  r_color_cyan_red: { min: -1, max: 1, step: 0.05 },
  r_color_magenta_green: { min: -1, max: 1, step: 0.05 },
  r_color_yellow_blue: { min: -1, max: 1, step: 0.05 },
  r_fake_shadows: { min: 0, max: 2, step: 1 },
  r_multimon_mode: { min: 0, max: 4, step: 1 },
  r_nv_stereo_mode: { min: 0, max: 2, step: 1 },
  g_traffic: { min: 0, max: 10, step: 0.1 },
  r_sun_shadow_texture_size: { min: 1024, max: 8192, step: 256 },
  r_buffer_page_size: { min: 10, max: 50, step: 1 },
  t_limit_fps: { min: 30, max: 360, step: 1 },
  r_mirror_view_distance: { min: 80, max: 400, step: 10 },
  g_lod_factor_traffic: { min: 1, max: 5, step: 0.1 },
  g_lod_factor_parked: { min: 1, max: 5, step: 0.1 },
  g_upshift_coef: { min: 0.1, max: 0.9, step: 0.01 },
  g_downshift_coef: { min: 0.1, max: 0.9, step: 0.01 },
  g_flyspeed: { min: 10, max: 500, step: 10 },
};
