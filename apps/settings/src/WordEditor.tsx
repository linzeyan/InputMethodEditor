// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

import { Button, Field, Input, makeStyles } from "@fluentui/react-components";
import Keymap, { Keymaps } from "./keymap";
import { useState } from "react";
import { DictionaryEntry } from "./DictionaryEditor";
import { invoke } from "@tauri-apps/api/core";

type OnWordEditorChangeData = {
  word: string;
  bopomofo: string;
  boost: number;
};

type WordEditorProps = {
  word?: string;
  bopomofo?: string;
  boost?: number;
  keymap?: Keymap;
  disabled?: boolean;
  onChange?: (data: OnWordEditorChangeData) => void;
};

const useStyles = makeStyles({
  root: {
    padding: "10px",
    display: "flex",
    flexDirection: "column",
  },
  keyboard: {
    marginTop: "20px",
    display: "flex",
    flexDirection: "column",
    gap: "5px",
  },
  keyboardRow: {
    display: "flex",
    flexDirection: "row",
    gap: "5px",
    ":nth-child(2)": {
      marginLeft: "10px",
    },
    ":nth-child(3)": {
      marginLeft: "20px",
    },
    ":nth-child(4)": {
      marginLeft: "30px",
    },
    ":nth-child(5)": {
      marginLeft: "40px",
    },
  },
  keycap: {
    minWidth: "35px",
    fontWeight: "normal",
    fontFamily: "標楷體",
  },
  bopomofo: {
    fontFamily: "標楷體",
  },
});

function WordEditor(props: WordEditorProps) {
  const styles = useStyles();
  const keymap = props.keymap || Keymaps.STD;
  const [entry, setEntry] = useState<DictionaryEntry>({
    word: props.word || "",
    bopomofo: props.bopomofo || "",
    boost: props.boost || 0,
  });

  const appendBopomofo = (value: string) => {
    setEntry({
      ...entry,
      bopomofo: entry.bopomofo + value,
    });
  };

  return (
    <div className={styles.root}>
      <Field label="字/詞">
        <Input
          disabled={props.disabled}
          value={entry.word}
          onChange={(_ev, data) => setEntry({ ...entry, word: data.value })}
        />
      </Field>
      <Field label="注音">
        <Input
          className={styles.bopomofo}
          disabled={props.disabled}
          value={entry.bopomofo}
          onChange={(_ev, data) =>
            invoke("map_bopomofo", { input: data.value }).then((ret) =>
              setEntry({ ...entry, bopomofo: ret as string }),
            )
          }
          placeholder="切換到英數模式可輸入注音，目前只支援標準鍵盤"
        />
      </Field>
      <Field label="常用度">
        <Input
          type="number"
          disabled={props.disabled}
          value={entry.boost.toString()}
          onChange={(_ev, data) =>
            setEntry({ ...entry, boost: parseInt(data.value) })
          }
        />
      </Field>
      <div className={styles.keyboard}>
        {keymap.layout.map((row, rowId) => (
          <div key={rowId} className={styles.keyboardRow}>
            {row.map((key) => (
              <Button
                key={key}
                disabled={props.disabled}
                className={styles.keycap}
                onClick={() => appendBopomofo(key)}
              >
                {key}
              </Button>
            ))}
          </div>
        ))}
        <div className={styles.keyboardRow}>
          <Button
            disabled={props.disabled}
            onClick={() => setEntry({ ...entry, bopomofo: "" })}
          >
            清空注音
          </Button>
          <Button
            disabled={props.disabled}
            style={{ flex: "1" }}
            onClick={() => appendBopomofo(" ")}
          ></Button>
          <Button
            disabled={props.disabled}
            appearance="primary"
            onClick={() => props.onChange && props.onChange(entry)}
          >
            確定
          </Button>
        </div>
      </div>
    </div>
  );
}

export default WordEditor;
export type { WordEditorProps, OnWordEditorChangeData };
