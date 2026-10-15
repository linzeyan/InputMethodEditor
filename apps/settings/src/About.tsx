// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

import { Button, makeStyles, Body1 } from "@fluentui/react-components";
import React, { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";

const useStyles = makeStyles({
  root: {
    overflow: "hidden",
  },
  row: {
    margin: "16px",
    display: "flex",
    flexDirection: "row",
    "@media (width < 20em)": {
      flexDirection: "column",
    },
  },
  column: {
    display: "flex",
    flexDirection: "column",
  },
  logo: {
    margin: 0,
    marginRight: "16px",
  },
  action: {
    width: "vw",
    margin: "10px",
    justifyContent: "end",
    display: "flex",
    flexDirection: "row",
    gap: "3px",
  },
});

function About() {
  const styles = useStyles();

  const [version, setVersion] = React.useState<string>("");

  useEffect(() => {
    invoke("app_version").then((v) => {
      setVersion(v as string);
    });
    // Made hidden, so it doesn't flash white before the page draws.
    getCurrentWindow().show();
  }, []);

  return (
    <div className={styles.root}>
      <div className={styles.row}>
        <div className={styles.column}>
          <figure className={styles.logo}>
            <img src="logo.svg" alt="InputMethodEditor logo" />
          </figure>
        </div>
        <div className={styles.column}>
          <Body1>InputMethodEditor</Body1>
          <Body1>版本：{version}</Body1>
          <Body1>授權方式：GPL-3.0-or-later</Body1>
          <Body1>
            專案首頁：
            <a
              href="#about"
              onClick={() =>
                openUrl("https://github.com/linzeyan/InputMethodEditor")
              }
            >
              GitHub
            </a>
          </Body1>
        </div>
      </div>
      <div className={styles.row}>
        <Body1>
          核心是新酷音（libchewing）。這個設定程式改自 windows-chewing-preferences，
          原作者為新酷音開發團隊（Chewing Project Authors），詳見
          <a href="#about" onClick={() => openUrl("https://chewing.im")}>
            https://chewing.im
          </a>
          。
        </Body1>
      </div>
      <div className={styles.action}>
        <Button onClick={() => getCurrentWindow().close()}>確定</Button>
      </div>
    </div>
  );
}

export default About;
