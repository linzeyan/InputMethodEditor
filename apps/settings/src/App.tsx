// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  Button,
  makeStyles,
  TabList,
  Tab,
  Checkbox,
  Field,
  Dropdown,
  Option,
  TabValue,
  SelectTabEvent,
  SelectTabData,
  SpinButton,
  Combobox,
  Input,
  Textarea,
  Text,
  CheckboxOnChangeData,
  OptionOnSelectData,
  SpinButtonChangeEvent,
  SpinButtonOnChangeData,
  InputOnChangeData,
  Slider,
  Tooltip,
  Select,
  SelectOnChangeData,
} from "@fluentui/react-components";
import { invoke } from "@tauri-apps/api/core";
import React, { ChangeEvent, useEffect } from "react";
import { ChewingTsfConfig, Config, KeybindValue } from "./config";
import { exit } from "@tauri-apps/plugin-process";
import { listen } from "@tauri-apps/api/event";
import { message, open, save } from "@tauri-apps/plugin-dialog";
import KeybindingTab from "./KeybindingTab";

type FontFamilyName = {
  name: string;
  display_name: string;
};

const useStyles = makeStyles({
  root: {
    position: "relative",
    padding: "5px 0px 0px 5px",
  },
  content: {
    margin: "16px",
    display: "flex",
    flexDirection: "row",
  },
  column: {
    flex: 1,
    display: "flex",
    flexDirection: "column",
    "& .fui-Field": {
      marginBottom: "12px",
    },
  },
  narrowColumn: {
    flex: 1,
    display: "flex",
    flexDirection: "column",
    "& .fui-Field": {
      gridTemplateColumns: "50% 1fr",
    },
  },
  action: {
    position: "absolute",
    bottom: "-4em",
    right: "16px",
    paddingBottom: "1em",
    display: "flex",
    flexDirection: "row",
    gap: "3px",
  },
  textarea: {
    height: "60vh",
  },
  texarea_inner: {
    maxHeight: "unset",
    fontFamily: "monospace",
  },
});

function sel_key_type_to_value(sel_key_type: number | undefined): string {
  switch (sel_key_type) {
    case 0:
      return "1234567890";
    case 1:
      return "asdfghjkl;";
    case 2:
      return "asdfzxcv89";
    case 3:
      return "asdfjkl789";
    case 4:
      return "aoeuhtn789";
    case 5:
      return "1234qweras";
    default:
      return "1234567890";
  }
}

function conv_engine_to_value(conv_engine: number | undefined): string {
  switch (conv_engine) {
    case 0:
      return "簡單注音";
    case 1:
      return "智慧選詞";
    case 2:
      return "模糊智慧選詞";
    default:
      return "智慧選詞";
  }
}

function simulate_english_layout_to_value(layout: number): string {
  switch (layout) {
    case 0:
      return "無";
    case 1:
      return "Dvorak";
    case 2:
      return "Carplx (QGMLWY)";
    case 3:
      return "Colemak";
    case 4:
      return "Colemak-DH ANSI";
    case 5:
      return "Colemak-DH Orth";
    case 6:
      return "Workman";
    default:
      return "無";
  }
}

function keyboard_layout_to_value(layout: number): string {
  switch (layout) {
    case 0:
      return "標準鍵盤";
    case 1:
      return "許氏鍵盤";
    case 2:
      return "IBM 鍵盤";
    case 3:
      return "精業鍵盤";
    case 4:
      return "倚天鍵盤";
    case 5:
      return "倚天 26 鍵";
    case 6:
      return "Dvorak (掃描碼變換)";
    case 7:
      return "Dvorak 許氏 (掃描碼變換)";
    case 8:
      return "大千 26 鍵";
    case 9:
      return "漢語拼音";
    case 10:
      return "台灣華語羅馬拼音";
    case 11:
      return "注音二式";
    case 12:
      return "Carplx (掃描碼變換)";
    case 13:
      return "Colemak-DH ANSI (掃描碼變換)";
    case 14:
      return "Colemak-DH Orth (掃描碼變換)";
    case 15:
      return "Workman (掃描碼變換)";
    case 16:
      return "Colemak (掃描碼變換)";
    default:
      return "標準鍵盤";
  }
}

function App() {
  const styles = useStyles();

  const [systemFonts, setSystemFonts] = React.useState<FontFamilyName[]>([]);
  const [selectedTab, setSelectedTab] = React.useState<TabValue>("1");
  const [config, setConfig] = React.useState<ChewingTsfConfig>();
  const [symbols_dat, setSymbolsDat] = React.useState<string>("");
  const [swkb_dat, setSwkbDat] = React.useState<string>("");
  const [custom_phrase_dat, setCustomPhraseDat] = React.useState<string>("");
  const [showAdvanced, setShowAdvanced] = React.useState<boolean>(false);

  useEffect(() => {
    const unlisten_import = listen("import", async () => {
      const file = await open({
        multiple: false,
        filters: [{ name: "TOML", extensions: ["toml"] }],
      });
      if (!file) {
        return;
      }
      invoke("import_config", { path: file })
        .then((value) => {
          const cfg = value as Config;
          setConfig(cfg.chewing_tsf);
          setSwkbDat(cfg.swkb_dat);
          setSymbolsDat(cfg.symbols_dat);
          setCustomPhraseDat(cfg.custom_phrase_dat);
        })
        .catch(async (e) => {
          await message("無法匯入設定檔，請確認檔案格式正確。\n\n" + e, {
            title: "錯誤",
            kind: "error",
          });
        });
    });
    const unlisten_export = listen("export", async () => {
      const file = await save({
        defaultPath: "InputMethodEditor 設定.toml",
        filters: [{ name: "TOML", extensions: ["toml"] }],
      });
      if (!file) {
        return;
      }
      invoke("export_config", {
        path: file,
        config: {
          chewing_tsf: config,
          symbols_dat,
          swkb_dat,
          custom_phrase_dat,
        },
      }).catch(async (e) => {
        await message("無法寫入檔案。\n\n" + e, {
          title: "錯誤",
          kind: "error",
        });
      });
    });
    return () => {
      unlisten_import.then((f) => f());
      unlisten_export.then((f) => f());
    };
  }, [config, symbols_dat, swkb_dat, custom_phrase_dat]);

  useEffect(() => {
    invoke("load_config").then((value) => {
      const config = value as Config;
      setConfig(config.chewing_tsf);
      setSwkbDat(config.swkb_dat);
      setSymbolsDat(config.symbols_dat);
      setCustomPhraseDat(config.custom_phrase_dat);
    });
  }, []);

  useEffect(() => {
    invoke("get_system_fonts").then((value) => {
      const fonts = value as FontFamilyName[];
      setSystemFonts(fonts);
    });
  }, []);

  const onTabSelect = (_event: SelectTabEvent, data: SelectTabData) => {
    setSelectedTab(data.value);
  };

  const setBooleanConfig = (
    event: React.ChangeEvent<HTMLInputElement>,
    data: CheckboxOnChangeData,
  ) => {
    let new_config = {
      ...config,
      [event.target.name]: data.checked,
    } as ChewingTsfConfig;
    // Disable conflicting configs
    if (new_config.enable_caps_lock) {
      new_config.switch_lang_with_shift = false;
    }
    setConfig(new_config);
  };

  const setNumberConfig =
    (name: string, fallback: number) =>
    (_event: SpinButtonChangeEvent, data: SpinButtonOnChangeData) => {
      const displayValue = parseInt(data.displayValue || fallback.toString());
      const value =
        data.value || (Number.isNaN(displayValue) ? fallback : displayValue);
      setConfig({
        ...config,
        [name]: value,
      } as ChewingTsfConfig);
    };

  const setStringConfig = (
    ev: React.ChangeEvent<HTMLInputElement>,
    data: InputOnChangeData,
  ) => {
    setConfig({
      ...config,
      [ev.target.name]: data.value,
    } as ChewingTsfConfig);
  };

  const setKeybind = (keybind: KeybindValue[]) => {
    setConfig({
      ...config,
      keybind,
    } as ChewingTsfConfig);
  };

  const save_config = () =>
    invoke("save_config", {
      config: {
        chewing_tsf: config,
        symbols_dat,
        swkb_dat,
        custom_phrase_dat,
      },
    }).catch(async (e) => {
      // Shown, then passed on so that 確定 leaves the window open.
      await message(String(e), { title: "錯誤", kind: "error" });
      throw e;
    });

  return (
    <form className={styles.root} name="configs">
      <TabList
        appearance="subtle"
        selectedValue={selectedTab}
        onTabSelect={onTabSelect}
      >
        <Tab value="1">打字行為</Tab>
        <Tab value="2">界面外觀</Tab>
        <Tab value="3">鍵盤設定</Tab>
        <Tab value="pinyin">拼音</Tab>
        <Tab value="keybind">自訂快捷鍵</Tab>
        <Tab value="4">特殊符號</Tab>
        <Tab value="5">快捷符號</Tab>
        <Tab value="phrases">自訂詞組</Tab>
      </TabList>
      {selectedTab === "1" && config && (
        <InputBehaviors
          config={config}
          styles={styles}
          showAdvanced={showAdvanced}
          setShowAdvanced={setShowAdvanced}
          setConfig={setConfig}
          setBooleanConfig={setBooleanConfig}
        />
      )}
      {selectedTab === "2" && config && (
        <Appearance
          config={config}
          styles={styles}
          systemFonts={systemFonts}
          showAdvanced={showAdvanced}
          setShowAdvanced={setShowAdvanced}
          setConfig={setConfig}
          setStringConfig={setStringConfig}
          setNumberConfig={setNumberConfig}
        />
      )}
      {selectedTab === "3" && config && (
        <Layout
          config={config}
          styles={styles}
          showAdvanced={showAdvanced}
          setShowAdvanced={setShowAdvanced}
          setConfig={setConfig}
        />
      )}
      {selectedTab === "pinyin" && config && (
        <Pinyin config={config} styles={styles} setConfig={setConfig} />
      )}
      {selectedTab === "4" && config && (
        <Symbols
          styles={styles}
          symbols_dat={symbols_dat}
          setSymbolsDat={setSymbolsDat}
        />
      )}
      {selectedTab === "5" && config && (
        <Shortcut styles={styles} swkb_dat={swkb_dat} setSwkbDat={setSwkbDat} />
      )}
      {selectedTab === "phrases" && config && (
        <Phrases
          styles={styles}
          custom_phrase_dat={custom_phrase_dat}
          setCustomPhraseDat={setCustomPhraseDat}
        />
      )}
      {selectedTab === "keybind" && config && (
        <KeybindingTab keybind={config.keybind} setKeybind={setKeybind} />
      )}
      <div className={styles.action}>
        <Button
          onClick={() => {
            save_config().then(() => exit(0));
          }}
        >
          確定
        </Button>
        <Button onClick={() => exit(0)}>取消</Button>
        <Button onClick={save_config}>套用</Button>
      </div>
    </form>
  );
}

const InputBehaviors = ({
  config,
  styles,
  showAdvanced,
  setShowAdvanced,
  setConfig,
  setBooleanConfig,
}) => (
  <div
    className={styles.content}
    role="tabpanel"
    aria-labelledby="InputBehaviors"
  >
    <div className={styles.column}>
      <Checkbox
        label="使用 Shift 快速切換中英文"
        name="switch_lang_with_shift"
        disabled={config?.enable_caps_lock}
        checked={config?.switch_lang_with_shift}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="使用 CapsLock 快速切換中英文"
        name="enable_caps_lock"
        checked={config?.enable_caps_lock}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="顯示中/英切換通知訊息"
        name="show_notification"
        checked={config?.show_notification}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="使用 Esc 清空編輯區字串"
        name="esc_clean_all_buf"
        checked={config?.esc_clean_all_buf}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="使用 Shift 輸入全形標點符號"
        name="full_shape_symbols"
        checked={config?.full_shape_symbols}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="按住 Shift 輸入大寫英文字母"
        name="upper_case_with_shift"
        checked={config?.upper_case_with_shift}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="Ctrl + 數字儲存游標前方的詞"
        name="add_phrase_forward"
        checked={config?.add_phrase_forward}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="啟用向後詞彙選詞模式"
        name="phrase_choice_rearward"
        checked={config?.phrase_choice_rearward}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="按住 Shift 輸入快捷符號"
        name="easy_symbols_with_shift"
        checked={config?.easy_symbols_with_shift}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="按住 Shift + Ctrl 輸入快捷符號"
        name="easy_symbols_with_shift_ctrl"
        checked={config?.easy_symbols_with_shift_ctrl}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="自動學習常用詞與新詞"
        name="enable_auto_learn"
        checked={config?.enable_auto_learn}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="依照常用程度排序手動選字選單"
        name="sort_candidates_by_frequency"
        checked={config?.sort_candidates_by_frequency}
        onChange={setBooleanConfig}
      />
    </div>
    <div className={styles.column}>
      <Checkbox
        label="使用方向鍵移動游標選字"
        name="cursor_cand_list"
        checked={config?.cursor_cand_list}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="按空白鍵叫出選字視窗"
        name="show_cand_with_space_key"
        checked={config?.show_cand_with_space_key}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="選字完畢自動跳到下一個字"
        name="advance_after_selection"
        checked={config?.advance_after_selection}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="預設以英文模式啟動"
        name="default_english"
        disabled={config?.enable_caps_lock}
        checked={config?.default_english}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="預設輸出簡體中文（或使用 Ctrl + F12 切換）"
        name="output_simp_chinese"
        checked={config?.output_simp_chinese}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="簡體改用大陸用語，如 軟體→软件"
        name="output_simp_vocabulary"
        disabled={!config?.output_simp_chinese}
        checked={config?.output_simp_vocabulary}
        onChange={setBooleanConfig}
      />
      <Checkbox
        label="以漢語拼音輸入（或使用 Ctrl + F11 切換）"
        name="pinyin"
        checked={config?.pinyin}
        onChange={setBooleanConfig}
      />
      <div style={{ marginLeft: "10px", marginTop: "10px" }}>
        <Field label="選字鍵：">
          <Dropdown
            value={sel_key_type_to_value(config?.sel_key_type)}
            selectedOptions={[config?.sel_key_type.toString() || ""]}
            onOptionSelect={(_ev, data) => {
              setConfig({
                ...config,
                sel_key_type: parseInt(data.optionValue || "0"),
              } as ChewingTsfConfig);
            }}
          >
            <Option value="0">1234567890</Option>
            <Option value="1">asdfghjkl;</Option>
            <Option value="2">asdfzxcv89</Option>
            <Option value="3">asdfjkl789</Option>
            <Option value="4">aoeuhtn789</Option>
            <Option value="5">1234qweras</Option>
          </Dropdown>
        </Field>
        <Field label="模式：">
          <Dropdown
            value={conv_engine_to_value(config?.conv_engine)}
            selectedOptions={[config?.conv_engine.toString() || ""]}
            onOptionSelect={(_ev, data) => {
              setConfig({
                ...config,
                conv_engine: parseInt(data.optionValue || "1"),
              } as ChewingTsfConfig);
            }}
          >
            <Option value="0">簡單注音</Option>
            <Option value="1">智慧選詞</Option>
            <Option value="2">模糊智慧選詞</Option>
          </Dropdown>
        </Field>
        <details
          open={showAdvanced}
          onToggle={(ev) => setShowAdvanced(ev.currentTarget.open)}
        >
          <summary style={{ marginBottom: "10px", cursor: "pointer" }}>
            進階設定...
          </summary>
          <Tooltip
            content="設定按住 Shift 鍵的時間長度，超過此時間視為長壓，取消切換中英模式。"
            relationship={"label"}
          >
            <Field
              label={`Shift 長壓敏感度：${config?.shift_key_sensitivity || 200} ms`}
            >
              <Slider
                value={config?.shift_key_sensitivity || 200}
                min={100}
                max={1000}
                step={100}
                onChange={(_ev, data) => {
                  setConfig({
                    ...config,
                    shift_key_sensitivity: data.value,
                  } as ChewingTsfConfig);
                }}
              />
            </Field>
          </Tooltip>
          <Tooltip
            content="調整 CapsLock 亮燈是中文還是英文。CapsLock 亮燈鎖定中文與各種軟體有較好的相容性。"
            relationship={"label"}
          >
            <Checkbox
              label="CapsLock 亮燈鎖定中文"
              disabled={!config?.enable_caps_lock}
              name="lock_chinese_on_caps_lock"
              checked={config?.lock_chinese_on_caps_lock}
              onChange={setBooleanConfig}
            />
          </Tooltip>
          <Checkbox
            label="設定英文模式等於關閉鍵盤"
            name="sync_lang_mode_openclose"
            checked={config?.sync_lang_mode_openclose}
            onChange={setBooleanConfig}
          />
        </details>
      </div>
    </div>
  </div>
);

const Appearance = ({
  config,
  styles,
  systemFonts,
  showAdvanced,
  setShowAdvanced,
  setConfig,
  setStringConfig,
  setNumberConfig,
}) => {
  const mapFontDisplayName = (name: string) => {
    let font = systemFonts.find((font: FontFamilyName) => font.name == name);
    if (font === undefined) {
      return name;
    }
    return font.display_name;
  };
  const apply_font_style = systemFonts.length < 500;
  const determineColorTheme = (config: ChewingTsfConfig): string => {
    if (!config) {
      return "custom";
    }
    if (
      config.font_fg_color == "EEEEEEFF" &&
      config.font_bg_color == "333333FF" &&
      config.cand_list_border_color == "222222FF" &&
      config.font_highlight_fg_color == "FFFFFFFF" &&
      config.font_highlight_bg_color == "111111FF" &&
      config.font_number_fg_color == "8C8DFFFF" &&
      config.notify_fg_color == "EEEEEEFF" &&
      config.notify_bg_color == "333333FF" &&
      config.notify_border_color == "222222FF"
    ) {
      return "dark";
    }
    if (
      config.font_fg_color == "000000FF" &&
      config.font_bg_color == "FAFAFAFF" &&
      config.cand_list_border_color == "D6D9DBFF" &&
      config.font_highlight_fg_color == "FFFFFFFF" &&
      config.font_highlight_bg_color == "000000FF" &&
      config.font_number_fg_color == "0000FFFF" &&
      config.notify_fg_color == "000000FF" &&
      config.notify_bg_color == "FCFBDAFF" &&
      config.notify_border_color == "D6D9DBFF"
    ) {
      return "light";
    }
    return "custom";
  };
  const selectColorTheme = (
    _ev: ChangeEvent<HTMLSelectElement>,
    data: SelectOnChangeData,
  ) => {
    if (data.value == "dark") {
      setConfig({
        ...config,
        font_fg_color: "EEEEEEFF",
        font_bg_color: "333333FF",
        cand_list_border_color: "222222FF",
        font_highlight_fg_color: "FFFFFFFF",
        font_highlight_bg_color: "111111FF",
        font_number_fg_color: "8C8DFFFF",
        notify_fg_color: "EEEEEEFF",
        notify_bg_color: "333333FF",
        notify_border_color: "222222FF",
      } as ChewingTsfConfig);
    }
    if (data.value == "light") {
      setConfig({
        ...config,
        font_fg_color: "000000FF",
        font_bg_color: "FAFAFAFF",
        cand_list_border_color: "D6D9DBFF",
        font_highlight_fg_color: "FFFFFFFF",
        font_highlight_bg_color: "000000FF",
        font_number_fg_color: "0000FFFF",
        notify_fg_color: "000000FF",
        notify_bg_color: "FCFBDAFF",
        notify_border_color: "D6D9DBFF",
      } as ChewingTsfConfig);
    }
    if (data.value == "custom") {
      setShowAdvanced(true);
    }
  };

  return (
    <div
      className={styles.content}
      role="tabpanel"
      aria-labelledby="Appearance"
    >
      <div className={styles.column}>
        <Field label="每列顯示後選字個數：">
          <SpinButton
            value={config?.cand_per_row}
            min={1}
            max={10}
            step={1}
            onChange={setNumberConfig("cand_per_row", 3)}
          />
        </Field>
        <Field label="每頁顯示後選字個數：">
          <SpinButton
            value={config?.cand_per_page}
            min={1}
            max={10}
            step={1}
            onChange={setNumberConfig("cand_per_page", 9)}
          />
        </Field>
        <Field label="選字及訊息視窗文字大小：">
          <SpinButton
            value={config?.font_size}
            step={1}
            onChange={setNumberConfig("font_size", 16)}
          />
        </Field>
        <Field label="選字視窗字型：">
          <Combobox
            value={mapFontDisplayName(config?.font_family)}
            selectedOptions={[config?.font_family || ""]}
            onOptionSelect={(_ev, data: OptionOnSelectData) => {
              setConfig({
                ...config,
                font_family: data.optionValue,
              } as ChewingTsfConfig);
            }}
          >
            {systemFonts.map((font: FontFamilyName) =>
              apply_font_style ? (
                <Option value={font.name} style={{ fontFamily: font.name }}>
                  {font.display_name}
                </Option>
              ) : (
                <Option value={font.name}>{font.display_name}</Option>
              ),
            )}
          </Combobox>
        </Field>
        <Field label="色彩佈景主題：">
          <Select
            value={determineColorTheme(config)}
            onChange={selectColorTheme}
          >
            <option value="dark">深色主題</option>
            <option value="light">淺色主題</option>
            <option value="custom">自訂</option>
          </Select>
        </Field>
        <details
          open={showAdvanced}
          onToggle={(ev) => setShowAdvanced(ev.currentTarget.open)}
        >
          <summary style={{ marginBottom: "10px", cursor: "pointer" }}>
            進階設定...
          </summary>
          <div className={styles.content}>
            <div className={styles.narrowColumn}>
              <Field label="文字顏色 RGB(A)" orientation="horizontal">
                <Input
                  name="font_fg_color"
                  value={config?.font_fg_color}
                  style={{ width: "8em" }}
                  onChange={setStringConfig}
                />
              </Field>
              <Field label="選字背景顏色 RGB(A)" orientation="horizontal">
                <Input
                  name="font_bg_color"
                  value={config?.font_bg_color}
                  style={{ width: "8em" }}
                  onChange={setStringConfig}
                />
              </Field>
              <Field label="選字邊框顏色 RGB(A)" orientation="horizontal">
                <Input
                  name="cand_list_border_color"
                  value={config?.cand_list_border_color}
                  style={{ width: "8em" }}
                  onChange={setStringConfig}
                />
              </Field>
              <Field label="焦點文字顏色 RGB(A)" orientation="horizontal">
                <Input
                  name="font_highlight_fg_color"
                  value={config?.font_highlight_fg_color}
                  style={{ width: "8em" }}
                  onChange={setStringConfig}
                />
              </Field>
              <Field label="焦點背景顏色 RGB(A)" orientation="horizontal">
                <Input
                  name="font_highlight_bg_color"
                  value={config?.font_highlight_bg_color}
                  style={{ width: "8em" }}
                  onChange={setStringConfig}
                />
              </Field>
              <Field label="數字顏色 RGB(A)" orientation="horizontal">
                <Input
                  name="font_number_fg_color"
                  value={config?.font_number_fg_color}
                  style={{ width: "8em" }}
                  onChange={setStringConfig}
                />
              </Field>
            </div>
            <div className={styles.narrowColumn}>
              <Field label="訊息文字顏色 RGB(A)" orientation="horizontal">
                <Input
                  name="notify_fg_color"
                  value={config?.notify_fg_color}
                  style={{ width: "8em" }}
                  onChange={setStringConfig}
                />
              </Field>
              <Field label="訊息背景顏色 RGB(A)" orientation="horizontal">
                <Input
                  name="notify_bg_color"
                  value={config?.notify_bg_color}
                  style={{ width: "8em" }}
                  onChange={setStringConfig}
                />
              </Field>
              <Field label="訊息邊框顏色 RGB(A)" orientation="horizontal">
                <Input
                  name="notify_border_color"
                  value={config?.notify_border_color}
                  style={{ width: "8em" }}
                  onChange={setStringConfig}
                />
              </Field>
            </div>
          </div>
        </details>
      </div>
    </div>
  );
};

const Layout = ({
  config,
  styles,
  showAdvanced,
  setShowAdvanced,
  setConfig,
}) => (
  <div className={styles.content} role="tabpanel" aria-labelledby="Layout">
    <div className={styles.column}>
      <Field label={`中文鍵盤布局：`}>
        <Dropdown
          value={keyboard_layout_to_value(config?.keyboard_layout || 0)}
          selectedOptions={[config?.keyboard_layout?.toString() || "0"]}
          onOptionSelect={(_ev, data) => {
            setConfig({
              ...config,
              keyboard_layout: parseInt(data.optionValue || "0"),
            } as ChewingTsfConfig);
          }}
        >
          <Option value="0">標準鍵盤</Option>
          <Option value="1">許氏鍵盤</Option>
          <Option value="2">IBM 鍵盤</Option>
          <Option value="3">精業鍵盤</Option>
          <Option value="4">倚天鍵盤</Option>
          <Option value="5">倚天 26 鍵</Option>
          <Option value="6">Dvorak (掃描碼變換)</Option>
          <Option value="7">Dvorak 許氏 (掃描碼變換)</Option>
          <Option value="8">大千 26 鍵</Option>
          <Option value="9">漢語拼音</Option>
          <Option value="10">台灣華語羅馬拼音</Option>
          <Option value="11">注音二式</Option>
          <Option value="12">Carplx (掃描碼變換)</Option>
          <Option value="16">Colemak (掃描碼變換)</Option>
          <Option value="13">Colemak-DH ANSI (掃描碼變換)</Option>
          <Option value="14">Colemak-DH Orth (掃描碼變換)</Option>
          <Option value="15">Workman (掃描碼變換)</Option>
        </Dropdown>
      </Field>
      <details
        open={showAdvanced}
        onToggle={(ev) => setShowAdvanced(ev.currentTarget.open)}
      >
        <summary style={{ marginBottom: "10px", cursor: "pointer" }}>
          進階設定...
        </summary>
        <Tooltip
          content="模擬英文鍵盤布局可能會讓某些網頁快捷鍵失效"
          relationship={"label"}
        >
          <Field label={`模擬英文鍵盤布局：`}>
            <Dropdown
              value={simulate_english_layout_to_value(
                config?.simulate_english_layout || 0,
              )}
              selectedOptions={[
                config?.simulate_english_layout?.toString() || "0",
              ]}
              onOptionSelect={(_ev, data) => {
                setConfig({
                  ...config,
                  simulate_english_layout: parseInt(data.optionValue || "0"),
                } as ChewingTsfConfig);
              }}
            >
              <Option value="0">無</Option>
              <Option value="1">Dvorak</Option>
              <Option value="2">Carplx (QGMLWY)</Option>
              <Option value="3">Colemak</Option>
              <Option value="4">Colemak-DH ANSI</Option>
              <Option value="5">Colemak-DH Orth</Option>
              <Option value="6">Workman</Option>
            </Dropdown>
          </Field>
        </Tooltip>
      </details>
    </div>
  </div>
);

// Bits of chewing's zhuyin::FUZZY_*.
const FUZZY_INITIALS: [string, number][] = [
  ["z = zh", 1 << 0],
  ["c = ch", 1 << 1],
  ["s = sh", 1 << 2],
  ["n = l", 1 << 3],
  ["f = h", 1 << 4],
  ["r = l", 1 << 5],
];
const FUZZY_FINALS: [string, number][] = [
  ["an = ang（含 ian = iang、uan = uang）", 1 << 6],
  ["en = eng", 1 << 7],
  ["in = ing", 1 << 8],
];

// Indexed by the config's shuangpin.
const SHUANGPIN = ["全拼", "小鶴雙拼", "自然碼雙拼", "微軟雙拼", "搜狗雙拼"];

const Pinyin = ({ config, styles, setConfig }) => {
  const fuzzy = ([label, bit]: [string, number]) => (
    <Checkbox
      key={bit}
      label={label}
      checked={(config.fuzzy_pinyin & bit) != 0}
      onChange={(_ev, data) =>
        setConfig({
          ...config,
          fuzzy_pinyin: data.checked
            ? config.fuzzy_pinyin | bit
            : config.fuzzy_pinyin & ~bit,
        } as ChewingTsfConfig)
      }
    />
  );
  return (
    <div role="tabpanel" aria-labelledby="Pinyin" style={{ margin: "16px" }}>
      <Field label="拼法：" style={{ marginBottom: "16px", width: "50%" }}>
        <Dropdown
          value={SHUANGPIN[config.shuangpin]}
          selectedOptions={[config.shuangpin.toString()]}
          onOptionSelect={(_ev, data) =>
            setConfig({
              ...config,
              shuangpin: parseInt(data.optionValue || "0"),
            } as ChewingTsfConfig)
          }
        >
          {SHUANGPIN.map((name, index) => (
            <Option key={index} value={index.toString()}>
              {name}
            </Option>
          ))}
        </Dropdown>
      </Field>
      <Text>
        模糊音：分不清的音，打哪一個都找得到兩邊的字，例如勾選 z = zh，打 zong
        也有「中」。只在拼音模式作用。
      </Text>
      <div className={styles.content} style={{ margin: "16px 0px" }}>
        <div className={styles.column}>
          <Field label="聲母">{FUZZY_INITIALS.map(fuzzy)}</Field>
        </div>
        <div className={styles.column}>
          <Field label="韻母">{FUZZY_FINALS.map(fuzzy)}</Field>
        </div>
      </div>
    </div>
  );
};

const Symbols = ({ styles, symbols_dat, setSymbolsDat }) => (
  <div className={styles.content} role="tabpanel" aria-labelledby="Symbols">
    <div className={styles.column}>
      <Field label="輸入中文時，按下 ` 鍵，會顯示下列的符號表：">
        <Textarea
          value={symbols_dat}
          className={styles.textarea}
          textarea={{ className: styles.texarea_inner }}
          onChange={(_ev, data) => setSymbolsDat(data.value)}
        />
      </Field>
      <Text>
        以上是符號表的設定檔，語法相當簡單：
        <br />
        每一行的內容都是：「分類名稱」＝「此分類下的所有符號」
        <br />
        您也可以一行只放一個符號，則該符號會被放在最上層選單。
      </Text>
    </div>
  </div>
);

const Shortcut = ({ styles, swkb_dat, setSwkbDat }) => (
  <div className={styles.content} role="tabpanel" aria-labelledby="Shortcut">
    <div className={styles.column}>
      <Field label="輸入中文時，按下 Shift 鍵（或 Ctrl + Shift）加英文字母即可快速輸入文字：">
        <Textarea
          value={swkb_dat}
          className={styles.textarea}
          textarea={{ className: styles.texarea_inner }}
          onChange={(_ev, data) => setSwkbDat(data.value)}
        />
      </Field>
      <Text>
        以上是符號表的設定檔，語法相當簡單：
        <br />
        每一行的內容都是：「大寫字母」＋「空格」＋「對應的符號或文字」。
      </Text>
    </div>
  </div>
);

const Phrases = ({ styles, custom_phrase_dat, setCustomPhraseDat }) => (
  <div className={styles.content} role="tabpanel" aria-labelledby="Phrases">
    <div className={styles.column}>
      <Field label="打縮寫再按空白鍵，就換成設定的文字（注音、拼音都可以用）：">
        <Textarea
          value={custom_phrase_dat}
          className={styles.textarea}
          textarea={{ className: styles.texarea_inner }}
          onChange={(_ev, data) => setCustomPhraseDat(data.value)}
        />
      </Field>
      <Text>
        每一行是：「縮寫」＋「空格」＋「文字」，縮寫只能用英文字母，例如：
        <br />
        addr 臺北市信義區市府路 1 號
        <br />
        縮寫要在沒有組字時開始打；和拼音相同的縮寫（例如 wo）會取代原本的字。#
        開頭的行是註解。
      </Text>
    </div>
  </div>
);

export default App;
