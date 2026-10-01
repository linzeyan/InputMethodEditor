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
  // Tooltip's trigger: a Checkbox hands pointer events to its hidden input,
  // which covers only the box and takes none while disabled.
  hint: {
    width: "fit-content",
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
      return "逐字選字";
    case 2:
      return "智慧選詞（可省略聲調，連打）";
    default:
      return "智慧選詞（每個字打聲調）";
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

// Every color field takes the same format.
const RGBA =
  "格式是十六進位 RRGGBBAA，最後兩位是不透明度（FF 為完全不透明），例如 000000FF 是黑色。";

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
        <Tab value="apps">各程式</Tab>
        <Tab value="2">界面外觀</Tab>
        <Tab value="3">鍵盤設定</Tab>
        <Tab value="pinyin">拼音</Tab>
        <Tab value="keybind">自訂快捷鍵</Tab>
        <Tab value="4">特殊符號</Tab>
        <Tab value="5">快捷符號</Tab>
        <Tab value="phrases">自訂詞組</Tab>
        <Tab value="update">更新</Tab>
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
      {selectedTab === "apps" && config && (
        <Apps config={config} styles={styles} setConfig={setConfig} />
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
      {selectedTab === "update" && config && (
        <Update
          config={config}
          styles={styles}
          setBooleanConfig={setBooleanConfig}
          setNumberConfig={setNumberConfig}
        />
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
      <Tooltip
        content="單獨按一下 Shift（不和其他鍵一起按）就切換中文／英文；按住超過「進階設定」的長壓時間則不切換。使用 CapsLock 切換時停用。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="使用 Shift 快速切換中英文"
            name="switch_lang_with_shift"
            disabled={config?.enable_caps_lock}
            checked={config?.switch_lang_with_shift}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="改用 CapsLock 切換中文／英文：中英由 CapsLock 燈號決定，亮燈是哪一種在「進階設定」調整。勾選後「使用 Shift 快速切換中英文」與「預設以英文模式啟動」停用。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="使用 CapsLock 快速切換中英文"
            name="enable_caps_lock"
            checked={config?.enable_caps_lock}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="切換中文／英文、正體／簡體、注音／拼音時，短暫顯示一個小視窗告訴你現在的模式。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="顯示中/英切換通知訊息"
            name="show_notification"
            checked={config?.show_notification}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="組字中按 Esc 清掉所有還沒送出的字。不勾選時，Esc 只清掉正在打的注音或拼音，已轉好的字保留。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="使用 Esc 清空編輯區字串"
            name="esc_clean_all_buf"
            checked={config?.esc_clean_all_buf}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="中文模式下 Shift＋標點鍵打出全形符號，例如 Shift＋1 是「！」；不勾選則是半形「!」。勾選「按住 Shift 輸入快捷符號」時一律是全形，這項停用。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="使用 Shift 輸入全形標點符號"
            name="full_shape_symbols"
            disabled={config?.easy_symbols_with_shift}
            checked={config?.full_shape_symbols}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="按 Shift＋空白鍵在半形與全形之間切換，一開始是半形。全形時英文字母、數字、符號和空白都打成全形，例如 ＡＢＣ１２３，中文、英文模式都是。不勾選時不能切換，只有中文模式的標點是全形（，。？）。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="使用 Shift + 空白鍵切換全形／半形"
            name="enable_fullwidth_toggle"
            checked={config?.enable_fullwidth_toggle}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="按 Shift＋空白鍵切換時，短暫顯示「全形」或「半形」。需先勾選「使用 Shift + 空白鍵切換全形／半形」。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="顯示全形／半形切換通知訊息"
            name="show_fullwidth_notification"
            disabled={!config?.enable_fullwidth_toggle}
            checked={config?.show_fullwidth_notification}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="中文模式下 Shift＋字母打出英文字母：勾選是大寫（A），不勾選是小寫（a）。勾選「按住 Shift 輸入快捷符號」時 Shift＋字母打的是快捷符號，這項停用。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="按住 Shift 輸入大寫英文字母"
            name="upper_case_with_shift"
            disabled={config?.easy_symbols_with_shift}
            checked={config?.upper_case_with_shift}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="組字中按 Ctrl＋2～9，把游標前面的 2～9 個字加入使用者詞庫，之後就能整個詞一起打出來；不勾選則是游標後面的字。例如打完「新酷音輸入法」按 Ctrl＋3，加入「輸入法」。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="Ctrl + 數字儲存游標前方的詞"
            name="add_phrase_forward"
            checked={config?.add_phrase_forward}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="決定組字中按 ↓ 選字時，候選是哪一段的詞。不勾選：從游標所在的字往後，游標在最後時只有最後一個字；勾選：到游標所在的字為止往前。例如打完「我覺得」直接按 ↓，不勾選列出「得、德…」，勾選列出「覺得」。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="啟用向後詞彙選詞模式"
            name="phrase_choice_rearward"
            checked={config?.phrase_choice_rearward}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="中文模式下 Shift＋字母打出「快捷符號」頁設定的符號，例如 Shift＋A 是「【」。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="按住 Shift 輸入快捷符號"
            name="easy_symbols_with_shift"
            checked={config?.easy_symbols_with_shift}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="中文模式下 Ctrl＋Shift＋字母打出「快捷符號」頁設定的符號。只勾這一項時，Shift＋字母仍然打英文字母。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="按住 Shift + Ctrl 輸入快捷符號"
            name="easy_symbols_with_shift_ctrl"
            checked={config?.easy_symbols_with_shift_ctrl}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="送出文字時記住你打過的詞和選過的字，之後整句轉換會優先用它們。關閉後不再學習，已學到的保留，可在「編輯使用者詞庫」管理。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="自動學習常用詞與新詞"
            name="enable_auto_learn"
            checked={config?.enable_auto_learn}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="按 ↓ 叫出的候選依常用程度排序（字典的統計加上你的使用習慣），常用的在前面；不勾選時依字典的固定順序。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="依照常用程度排序手動選字選單"
            name="sort_candidates_by_frequency"
            checked={config?.sort_candidates_by_frequency}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
    </div>
    <div className={styles.column}>
      <Tooltip
        content="選字視窗出現時，用方向鍵移動反白、按 Enter 選取；Ctrl＋Delete 忘記詞也需要這一項，作用在反白的那一個。不勾選時用選字鍵選字。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="使用方向鍵移動游標選字"
            name="cursor_cand_list"
            checked={config?.cursor_cand_list}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="注音組字中，打完一個字後按空白鍵叫出選字視窗，和 ↓ 相同。不勾選時，空白鍵把游標前的字改成一聲，連按兩下才打出空白。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="按空白鍵叫出選字視窗"
            name="show_cand_with_space_key"
            checked={config?.show_cand_with_space_key}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="按 ↓ 選好一個詞後，游標自動移到它後面，可以接著選下一段；不勾選時游標留在原處。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="選字完畢自動跳到下一個字"
            name="advance_after_selection"
            checked={config?.advance_after_selection}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="每個程式會記住上次切到的中文或英文，下次開啟時沿用；這項決定還沒切換過的程式從哪個模式開始。「各程式」頁列出的程式照那裡的設定。使用 CapsLock 切換中英文時由燈號決定，這項停用。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="預設以英文模式啟動"
            name="default_english"
            disabled={config?.enable_caps_lock}
            checked={config?.default_english}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="把打出的字轉成簡體字形，例如 軟體→软体。隨時可用 Ctrl＋F12 或右鍵選單切換，所有程式共用。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="預設輸出簡體中文（或使用 Ctrl + F12 切換）"
            name="output_simp_chinese"
            checked={config?.output_simp_chinese}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="簡體輸出時連用語一起換成大陸說法，例如 軟體→软件、網際網路→互联网；不勾選只轉字形。需先勾選簡體輸出。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="簡體改用大陸用語，如 軟體→软件"
            name="output_simp_vocabulary"
            disabled={!config?.output_simp_chinese}
            checked={config?.output_simp_vocabulary}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <Tooltip
        content="改用漢語拼音輸入：直接連打、不用聲調，例如 nihao 是「你好」。模糊音與雙拼在「拼音」頁設定。隨時可用 Ctrl＋F11 切換，所有程式共用。"
        relationship="description"
      >
        <div className={styles.hint}>
          <Checkbox
            label="以漢語拼音輸入（或使用 Ctrl + F11 切換）"
            name="pinyin"
            checked={config?.pinyin}
            onChange={setBooleanConfig}
          />
        </div>
      </Tooltip>
      <div style={{ marginLeft: "10px", marginTop: "10px" }}>
        <Tooltip
          content="選字視窗裡第 1、2、3… 個候選用哪些鍵選取。選 asdf 開頭的組合，手不用離開主鍵區。"
          relationship="description"
        >
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
        </Tooltip>
        <Tooltip
          content="注音怎麼轉成中文（拼音一律可省略聲調）。每個字打聲調：依整句自動挑字，一聲按空白。可省略聲調：ㄋㄧㄏㄠ 就是「你好」，只打聲母也行，ㄐㄊㄊㄑㄏㄏ 是「今天天氣很好」，最後一個字按空白、Enter 或標點結束；ˊˇˋ˙ 照樣有效，但空白不再限定一聲，例如 ㄉㄠ 加空白可能是「到」而不是「刀」。逐字選字：每打完一個字就跳出候選，不做整句轉換。"
          relationship="description"
        >
          <Field label="注音模式：">
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
              <Option value="1">智慧選詞（每個字打聲調）</Option>
              <Option value="2">智慧選詞（可省略聲調，連打）</Option>
              <Option value="0">逐字選字</Option>
            </Dropdown>
          </Field>
        </Tooltip>
        <details
          open={showAdvanced}
          onToggle={(ev) => setShowAdvanced(ev.currentTarget.open)}
        >
          <summary style={{ marginBottom: "10px", cursor: "pointer" }}>
            進階設定...
          </summary>
          <Tooltip
            content="單獨按住 Shift 超過這個時間才放開，視為長壓，不切換中英文。"
            relationship="description"
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
            content="勾選時 CapsLock 亮燈是中文、熄燈是英文；不勾選則相反。亮燈鎖定中文與各種軟體的相容性較好。需先勾選「使用 CapsLock 快速切換中英文」。"
            relationship="description"
          >
            <div className={styles.hint}>
              <Checkbox
                label="CapsLock 亮燈鎖定中文"
                disabled={!config?.enable_caps_lock}
                name="lock_chinese_on_caps_lock"
                checked={config?.lock_chinese_on_caps_lock}
                onChange={setBooleanConfig}
              />
            </div>
          </Tooltip>
          <Tooltip
            content="切到英文模式時，也把 Windows 的輸入法開關設為關閉，和按 Ctrl＋Space 關閉輸入法相同。只有依這個開關判斷輸入法狀態的程式才需要，一般不用勾選。"
            relationship="description"
          >
            <div className={styles.hint}>
              <Checkbox
                label="設定英文模式等於關閉鍵盤"
                name="sync_lang_mode_openclose"
                checked={config?.sync_lang_mode_openclose}
                onChange={setBooleanConfig}
              />
            </div>
          </Tooltip>
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
        <Tooltip
          content="選字視窗每一列排幾個候選，排不下的換到下一列。設為 1 就是直的一排。"
          relationship="description"
        >
          <Field label="每列顯示後選字個數：">
            <SpinButton
              value={config?.cand_per_row}
              min={1}
              max={10}
              step={1}
              onChange={setNumberConfig("cand_per_row", 3)}
            />
          </Field>
        </Tooltip>
        <Tooltip
          content="選字視窗一頁最多幾個候選，其餘的要翻頁；也決定用到幾個選字鍵。"
          relationship="description"
        >
          <Field label="每頁顯示後選字個數：">
            <SpinButton
              value={config?.cand_per_page}
              min={1}
              max={10}
              step={1}
              onChange={setNumberConfig("cand_per_page", 9)}
            />
          </Field>
        </Tooltip>
        <Tooltip
          content="選字視窗和切換模式時的訊息視窗的文字大小。"
          relationship="description"
        >
          <Field label="選字及訊息視窗文字大小：">
            <SpinButton
              value={config?.font_size}
              step={1}
              onChange={setNumberConfig("font_size", 16)}
            />
          </Field>
        </Tooltip>
        <Tooltip
          content="選字視窗和訊息視窗使用的字型。"
          relationship="description"
        >
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
        </Tooltip>
        <Tooltip
          content="選字視窗和訊息視窗的配色。選「自訂」可在進階設定逐一調整每個顏色。"
          relationship="description"
        >
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
        </Tooltip>
        <details
          open={showAdvanced}
          onToggle={(ev) => setShowAdvanced(ev.currentTarget.open)}
        >
          <summary style={{ marginBottom: "10px", cursor: "pointer" }}>
            進階設定...
          </summary>
          <div className={styles.content}>
            <div className={styles.narrowColumn}>
              <Tooltip
                content={`候選字的文字顏色。${RGBA}`}
                relationship="description"
              >
                <Field label="文字顏色 RGB(A)" orientation="horizontal">
                  <Input
                    name="font_fg_color"
                    value={config?.font_fg_color}
                    style={{ width: "8em" }}
                    onChange={setStringConfig}
                  />
                </Field>
              </Tooltip>
              <Tooltip
                content={`選字視窗的背景顏色。${RGBA}`}
                relationship="description"
              >
                <Field label="選字背景顏色 RGB(A)" orientation="horizontal">
                  <Input
                    name="font_bg_color"
                    value={config?.font_bg_color}
                    style={{ width: "8em" }}
                    onChange={setStringConfig}
                  />
                </Field>
              </Tooltip>
              <Tooltip
                content={`選字視窗的邊框顏色。${RGBA}`}
                relationship="description"
              >
                <Field label="選字邊框顏色 RGB(A)" orientation="horizontal">
                  <Input
                    name="cand_list_border_color"
                    value={config?.cand_list_border_color}
                    style={{ width: "8em" }}
                    onChange={setStringConfig}
                  />
                </Field>
              </Tooltip>
              <Tooltip
                content={`用方向鍵反白的那個候選的文字顏色。${RGBA}`}
                relationship="description"
              >
                <Field label="焦點文字顏色 RGB(A)" orientation="horizontal">
                  <Input
                    name="font_highlight_fg_color"
                    value={config?.font_highlight_fg_color}
                    style={{ width: "8em" }}
                    onChange={setStringConfig}
                  />
                </Field>
              </Tooltip>
              <Tooltip
                content={`用方向鍵反白的那個候選的背景顏色。${RGBA}`}
                relationship="description"
              >
                <Field label="焦點背景顏色 RGB(A)" orientation="horizontal">
                  <Input
                    name="font_highlight_bg_color"
                    value={config?.font_highlight_bg_color}
                    style={{ width: "8em" }}
                    onChange={setStringConfig}
                  />
                </Field>
              </Tooltip>
              <Tooltip
                content={`候選前面選字鍵（1、2、3…）的顏色。${RGBA}`}
                relationship="description"
              >
                <Field label="數字顏色 RGB(A)" orientation="horizontal">
                  <Input
                    name="font_number_fg_color"
                    value={config?.font_number_fg_color}
                    style={{ width: "8em" }}
                    onChange={setStringConfig}
                  />
                </Field>
              </Tooltip>
            </div>
            <div className={styles.narrowColumn}>
              <Tooltip
                content={`切換模式時訊息視窗的文字顏色。${RGBA}`}
                relationship="description"
              >
                <Field label="訊息文字顏色 RGB(A)" orientation="horizontal">
                  <Input
                    name="notify_fg_color"
                    value={config?.notify_fg_color}
                    style={{ width: "8em" }}
                    onChange={setStringConfig}
                  />
                </Field>
              </Tooltip>
              <Tooltip
                content={`切換模式時訊息視窗的背景顏色。${RGBA}`}
                relationship="description"
              >
                <Field label="訊息背景顏色 RGB(A)" orientation="horizontal">
                  <Input
                    name="notify_bg_color"
                    value={config?.notify_bg_color}
                    style={{ width: "8em" }}
                    onChange={setStringConfig}
                  />
                </Field>
              </Tooltip>
              <Tooltip
                content={`切換模式時訊息視窗的邊框顏色。${RGBA}`}
                relationship="description"
              >
                <Field label="訊息邊框顏色 RGB(A)" orientation="horizontal">
                  <Input
                    name="notify_border_color"
                    value={config?.notify_border_color}
                    style={{ width: "8em" }}
                    onChange={setStringConfig}
                  />
                </Field>
              </Tooltip>
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
      <Tooltip
        content="注音符號在鍵盤上的排列，照你習慣的注音鍵盤選，只在注音模式作用。標「掃描碼變換」的，是給 Windows 英文鍵盤設成 Dvorak、Colemak 等配置的人：注音仍在標準（或許氏）鍵盤的實體位置。「漢語拼音」「台灣華語羅馬拼音」「注音二式」是新酷音內建的逐字拼音，每個字要打聲調或空白；要連打拼音請改用「打字行為」的「以漢語拼音輸入」。"
        relationship="description"
      >
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
      </Tooltip>
      <details
        open={showAdvanced}
        onToggle={(ev) => setShowAdvanced(ev.currentTarget.open)}
      >
        <summary style={{ marginBottom: "10px", cursor: "pointer" }}>
          進階設定...
        </summary>
        <Tooltip
          content="Windows 的鍵盤維持 QWERTY，英文模式時由輸入法把按鍵換成 Dvorak 等配置的字母。可能會讓某些網頁的快捷鍵失效。"
          relationship="description"
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
      <Tooltip
        content="全拼：照拼音的字母打，例如 shuang。雙拼：每個字固定兩鍵（聲母鍵＋韻母鍵），打完第二鍵就轉成中文，鍵位依方案而定。只在拼音模式作用。"
        relationship="description"
      >
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
      </Tooltip>
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

const Update = ({ config, styles, setBooleanConfig, setNumberConfig }) => (
  <div role="tabpanel" aria-labelledby="Update" style={{ margin: "16px" }}>
    <Tooltip
      content="登入後和每天中午，看 GitHub 上有沒有新版本；有新版本時會跳出詢問，同意才下載安裝。取消勾選就不再連到 GitHub，要更新時自己下載新版的安裝檔。"
      relationship="description"
    >
      <div className={styles.hint}>
        <Checkbox
          label="自動檢查更新"
          name="check_update"
          checked={config.check_update}
          onChange={setBooleanConfig}
        />
      </div>
    </Tooltip>
    <Tooltip
      content="每次登入 Windows（包括開機後登入）約 5 分鐘時查看一次，不管距離上次多久；同一次登入之後仍照下面的間隔。"
      relationship="description"
    >
      <div className={styles.hint}>
        <Checkbox
          label="登入時也檢查一次"
          name="check_update_at_logon"
          disabled={!config.check_update}
          checked={config.check_update_at_logon}
          onChange={setBooleanConfig}
        />
      </div>
    </Tooltip>
    <Tooltip
      content="距離上次查看要滿這麼多天才會再查，1 到 30 天。"
      relationship="description"
    >
      <Field label="每隔幾天檢查一次新版本：" style={{ width: "50%" }}>
        <SpinButton
          value={config.update_check_days}
          min={1}
          max={30}
          step={1}
          disabled={!config.check_update}
          onChange={setNumberConfig("update_check_days", 7)}
        />
      </Field>
    </Tooltip>
  </div>
);

const Apps = ({ config, styles, setConfig }) => (
  <div role="tabpanel" aria-labelledby="Apps" style={{ margin: "16px" }}>
    <div style={{ display: "flex", gap: "16px" }}>
      {[
        ["english_apps", "開啟或切換到這些程式時用英數："],
        ["chinese_apps", "開啟或切換到這些程式時用中文："],
      ].map(([name, label]) => (
        <Field key={name} label={label} style={{ flex: 1 }}>
          <Textarea
            value={config[name]}
            style={{ height: "50vh" }}
            textarea={{ className: styles.texarea_inner }}
            onChange={(_ev, data) => setConfig({ ...config, [name]: data.value })}
          />
        </Field>
      ))}
    </div>
    <Text>
      每行一個程式的執行檔名稱，例如 chrome.exe、Telegram.exe，可在工作管理員的「詳細資料」頁找到；大小寫不拘，.exe
      可省略。在程式裡照樣可以切換，離開再回來就換回這裡的設定。沒列出的程式照舊：記住上次的模式，或依
      CapsLock 燈號。用 CapsLock 切換中英文時，會自動按一下 CapsLock 讓燈號對上。
    </Text>
  </div>
);

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
