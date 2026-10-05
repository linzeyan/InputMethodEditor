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
  // Heads the column of checkboxes it acts on.
  all: {
    marginTop: "8px",
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
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
 * in `listed` show as already added; those in `other` can't be picked, as a
 * program goes in one list only.
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
  // 全選 takes what the search shows; picks hidden by it stay as they are.
  const pickable = (shown ?? [])
    .map((program) => program.exe)
    .filter((exe) => !listed.has(appKey(exe)) && !other.has(appKey(exe)));
  const pickedShown = pickable.filter((exe) => picked.includes(exe)).length;
  const toggleAll = (on: boolean) =>
    setPicked([
      ...picked.filter((exe) => !pickable.includes(exe)),
      ...(on ? pickable : []),
    ]);

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
            <div className={styles.all}>
              <Checkbox
                label="全選"
                disabled={pickable.length === 0}
                checked={
                  pickedShown === 0
                    ? false
                    : pickedShown === pickable.length
                      ? true
                      : "mixed"
                }
                onChange={(_ev, data) => toggleAll(data.checked === true)}
              />
            </div>
            <div className={styles.list}>
              {error && <Text>無法讀取程式清單：{error}</Text>}
              {!programs && !error && <Spinner label="正在讀取程式清單…" />}
              {shown?.map((program) => {
                const key = appKey(program.exe);
                const added = listed.has(key);
                const taken = other.has(key);
                const note = added
                  ? " · 已在清單中"
                  : taken
                    ? ` · ${otherLabel}`
                    : "";
                return (
                  <Checkbox
                    key={key}
                    checked={added || picked.includes(program.exe)}
                    disabled={added || taken}
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
