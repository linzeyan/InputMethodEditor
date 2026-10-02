// SPDX-License-Identifier: GPL-3.0-or-later

import {
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  SearchBox,
  Spinner,
  Text,
  makeStyles,
  tokens,
} from "@fluentui/react-components";
import { AppGenericRegular } from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import React from "react";

type Program = {
  name: string;
  exe: string;
  /** A PNG data URL, if the program has an icon. */
  icon: string | null;
};

/** A listed program as the IME matches it: in any case, with or without .exe. */
export const appKey = (line: string) => {
  const name = line.trim().toLowerCase();
  return name.endsWith(".exe") ? name : `${name}.exe`;
};

const useStyles = makeStyles({
  list: {
    height: "50vh",
    overflowY: "auto",
    marginTop: "8px",
    display: "flex",
    flexDirection: "column",
  },
  label: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
  },
  icon: {
    width: "20px",
    height: "20px",
    flexShrink: 0,
  },
  exe: {
    color: tokens.colorNeutralForeground3,
  },
});

/**
 * Picks programs from the Start menu's and those with a window open. The ones
 * in `listed` show as already added; picking one in `other` moves it here.
 */
export const ProgramPicker = ({
  title,
  listed,
  other,
  otherLabel,
  onAdd,
  onClose,
}: {
  title: string;
  listed: Set<string>;
  other: Set<string>;
  otherLabel: string;
  onAdd: (exes: string[]) => void;
  onClose: () => void;
}) => {
  const styles = useStyles();
  const [programs, setPrograms] = React.useState<Program[]>();
  const [error, setError] = React.useState("");
  const [query, setQuery] = React.useState("");
  const [picked, setPicked] = React.useState<string[]>([]);

  React.useEffect(() => {
    invoke<Program[]>("list_programs")
      .then((list) =>
        setPrograms(
          list.sort((a, b) => a.name.localeCompare(b.name, "zh-Hant")),
        ),
      )
      .catch((e) => setError(String(e)));
  }, []);

  const needle = query.trim().toLowerCase();
  const shown = programs?.filter(
    (program) =>
      program.name.toLowerCase().includes(needle) ||
      program.exe.toLowerCase().includes(needle),
  );
  const toggle = (exe: string, on: boolean) =>
    setPicked(on ? [...picked, exe] : picked.filter((p) => p !== exe));

  return (
    <Dialog open onOpenChange={(_ev, data) => data.open || onClose()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{title}</DialogTitle>
          <DialogContent>
            <SearchBox
              placeholder="搜尋名稱或執行檔"
              value={query}
              style={{ width: "100%" }}
              onChange={(_ev, data) => setQuery(data.value)}
            />
            <div className={styles.list}>
              {error && <Text>無法讀取程式清單：{error}</Text>}
              {!programs && !error && <Spinner label="正在讀取程式清單…" />}
              {shown?.map((program) => {
                const key = appKey(program.exe);
                const added = listed.has(key);
                const note = added
                  ? " · 已在清單中"
                  : other.has(key)
                    ? ` · ${otherLabel}`
                    : "";
                return (
                  <Checkbox
                    key={key}
                    checked={added || picked.includes(program.exe)}
                    disabled={added}
                    onChange={(_ev, data) =>
                      toggle(program.exe, !!data.checked)
                    }
                    label={
                      <span className={styles.label}>
                        {program.icon ? (
                          <img
                            className={styles.icon}
                            src={program.icon}
                            alt=""
                          />
                        ) : (
                          <AppGenericRegular className={styles.icon} />
                        )}
                        <span>
                          {program.name}{" "}
                          <Text size={200} className={styles.exe}>
                            {program.exe}
                            {note}
                          </Text>
                        </span>
                      </span>
                    }
                  />
                );
              })}
            </div>
          </DialogContent>
          <DialogActions>
            <Button
              appearance="primary"
              disabled={picked.length === 0}
              onClick={() => onAdd(picked)}
            >
              {picked.length ? `加入 ${picked.length} 個` : "加入"}
            </Button>
            <Button onClick={onClose}>取消</Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
};
