// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

export type ChewingTsfConfig = {
  switch_lang_with_shift: boolean;
  shift_key_sensitivity: number;
  enable_caps_lock: boolean;
  lock_chinese_on_caps_lock: boolean;
  show_notification: boolean;
  enable_auto_learn: boolean;
  esc_clean_all_buf: boolean;
  full_shape_symbols: boolean;
  upper_case_with_shift: boolean;
  add_phrase_forward: boolean;
  phrase_choice_rearward: boolean;
  easy_symbols_with_shift: boolean;
  easy_symbols_with_shift_ctrl: boolean;
  cursor_cand_list: boolean;
  sort_candidates_by_frequency: boolean;
  show_cand_with_space_key: boolean;
  advance_after_selection: boolean;
  default_english: boolean;
  output_simp_chinese: boolean;
  output_simp_vocabulary: boolean;
  pinyin: boolean;
  fuzzy_pinyin: number;
  shuangpin: number;
  sel_key_type: number;
  conv_engine: number;
  cand_per_row: number;
  cand_per_page: number;
  font_size: number;
  font_family: string;
  font_fg_color: string;
  font_bg_color: string;
  font_highlight_fg_color: string;
  font_highlight_bg_color: string;
  font_number_fg_color: string;
  cand_list_border_color: string;
  notify_fg_color: string;
  notify_bg_color: string;
  notify_border_color: string;
  keyboard_layout: number;
  simulate_english_layout: number;
  sync_lang_mode_openclose: boolean;
  keybind: [KeybindValue];
};

export type KeybindValue = {
  key: string;
  action: string;
  param: string;
};

export type Config = {
  chewing_tsf: ChewingTsfConfig;
  symbols_dat: string;
  swkb_dat: string;
};
