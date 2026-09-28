// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import About from "./About";
import DictionaryEditor from "./DictionaryEditor";
import { invoke } from "@tauri-apps/api/core";
import { getAllWindows } from "@tauri-apps/api/window";
import { usePrefersColorScheme } from "./theme";
import {
  FluentProvider,
  webDarkTheme,
  webLightTheme,
} from "@fluentui/react-components";

getAllWindows().then(async (wins) => {
  let version = await invoke("app_version") as string;
  for (const w of wins) {
    let title = await w.title();
    if (!title.includes(version)) {
      w.setTitle(`${title} (${version})`);
    }
  }
});

function Root({ page }: { page: string }) {
  const isDarkTheme = usePrefersColorScheme();

  return (
    <React.StrictMode>
      <FluentProvider theme={isDarkTheme ? webDarkTheme : webLightTheme}>
        {location.hash == "" &&
          (page == "dictionary" ? <DictionaryEditor /> : <App />)}
        {location.hash == "#about" && <About />}
      </FluentProvider>
    </React.StrictMode>
  );
}

invoke("start_page").then((page) =>
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <Root page={page as string} />,
  ),
);
