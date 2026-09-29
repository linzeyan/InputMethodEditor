// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  Body1Strong,
  Button,
  Divider,
  Input,
  InputOnChangeData,
  makeStyles,
  Select,
  SelectOnChangeData,
  Tooltip,
} from "@fluentui/react-components";
import { DeleteRegular } from "@fluentui/react-icons";
import { KeybindValue } from "./config";
import { ChangeEvent, MouseEvent } from "react";

const useStyles = makeStyles({
  root: {
    height: "500px",
    display: "flex",
    flexDirection: "column",
    alignItems: "start",
    gap: "10px",
    overflow: "auto",
    padding: "25px 0px 0px 5px",
  },
  row: {
    display: "flex",
    flexDirection: "row",
    justifyContent: "end",
    width: "90%",
  },
  action: {
    alignSelf: "center",
    width: "200px",
  },
  param: {
    width: "200px",
  },
  key: {
    width: "200px",
    "& button": {
      minWidth: "190px",
    },
  },
  delete: {
    width: "32px",
  },
});

type KeybindingTabProps = {
  keybind: KeybindValue[];
  setKeybind: (keybind: KeybindValue[]) => void;
};

function KeybindingTab(props: KeybindingTabProps) {
  const styles = useStyles();
  const addKeyBinding = (_ev: MouseEvent<HTMLButtonElement>) => {
    props.setKeybind([
      ...props.keybind,
      { action: "text", param: "", key: "" },
    ]);
  };
  const setKeyBindingAction = (
    ev: ChangeEvent<HTMLSelectElement>,
    data: SelectOnChangeData,
  ) => {
    console.log(ev);
    let maybeIndex = ev.currentTarget.dataset.index;
    if (maybeIndex == null) {
      return;
    }
    let index = parseInt(maybeIndex);
    props.setKeybind(
      props.keybind.map((kb, i) => {
        if (index == i) {
          kb.action = data.value;
        }
        return kb;
      }),
    );
  };
  const setKeyBindingParam = (
    ev: ChangeEvent<HTMLInputElement>,
    data: InputOnChangeData,
  ) => {
    let maybeIndex = ev.currentTarget.dataset.index;
    if (maybeIndex == null) {
      return;
    }
    let index = parseInt(maybeIndex);
    props.setKeybind(
      props.keybind.map((kb, i) => {
        if (index == i) {
          kb.param = data.value;
        }
        return kb;
      }),
    );
  };
  const setKeyBindingKey = (
    ev: ChangeEvent<HTMLInputElement>,
    data: InputOnChangeData,
  ) => {
    let maybeIndex = ev.currentTarget.dataset.index;
    if (maybeIndex == null) {
      return;
    }
    let index = parseInt(maybeIndex);
    props.setKeybind(
      props.keybind.map((kb, i) => {
        if (index == i) {
          kb.key = data.value;
        }
        return kb;
      }),
    );
  };
  const deleteKeyBinding = (ev: MouseEvent<HTMLButtonElement>) => {
    let index = ev.currentTarget.dataset.index;
    if (index == null) {
      return;
    }
    let keybind = [
      ...props.keybind.slice(0, parseInt(index)),
      ...props.keybind.slice(parseInt(index) + 1),
    ];
    props.setKeybind(keybind);
  };
  return (
    <div className={styles.root}>
      <div className={styles.row}>
        <div className={styles.action}>
          <Tooltip
            content="按下快捷鍵時做什麼。「輸入文字或符號」打出參數裡的文字；「忘掉選擇中詞彙」要在選字視窗用方向鍵反白一個詞時按，讓它不再優先出現。"
            relationship="description"
          >
            <Body1Strong>動作</Body1Strong>
          </Tooltip>
        </div>
        <div className={styles.param}>
          <Tooltip
            content="只有「輸入文字或符號」用得到：按下快捷鍵時打出的文字。"
            relationship="description"
          >
            <Body1Strong>參數</Body1Strong>
          </Tooltip>
        </div>
        <div className={styles.key}>
          <Tooltip
            content="用 + 連接修飾鍵和按鍵，例如 Ctrl+F12、Ctrl+Delete。修飾鍵：Ctrl、Shift、Alt；按鍵：單一字元、F1～F12、Esc、Tab、Enter、Space、Delete、Backspace、Home、End、CapsLock。"
            relationship="description"
          >
            <Body1Strong>快捷鍵</Body1Strong>
          </Tooltip>
        </div>
        <div className={styles.delete}>
          <Body1Strong>刪除</Body1Strong>
        </div>
      </div>
      <Divider style={{ flexGrow: 0 }} />
      {props.keybind.map((kb, i) => (
        <div className={styles.row}>
          <div className={styles.action}>
            <Select
              value={kb.action}
              onChange={setKeyBindingAction}
              data-index={i}
            >
              <option value="text">輸入文字或符號</option>
              <option value="toggle_simplified_chinese">
                切換輸出簡體中文
              </option>
              <option value="toggle_hsu_keyboard">切換標準或許氏鍵盤</option>
              <option value="toggle_pinyin">切換注音或拼音</option>
              <option value="selecting_unlearn_phrase">忘掉選擇中詞彙</option>
            </Select>
          </div>
          <div className={styles.param}>
            <Input
              type="text"
              value={kb.param}
              onChange={setKeyBindingParam}
              data-index={i}
            ></Input>
          </div>
          <div className={styles.key}>
            <Input
              type="text"
              value={kb.key}
              onChange={setKeyBindingKey}
              data-index={i}
            ></Input>
          </div>
          <div className={styles.delete}>
            <Button
              icon={<DeleteRegular />}
              onClick={deleteKeyBinding}
              data-index={i}
            ></Button>
          </div>
        </div>
      ))}
      <Button style={{ width: "90%" }} onClick={addKeyBinding}>
        +
      </Button>
    </div>
  );
}

export default KeybindingTab;
