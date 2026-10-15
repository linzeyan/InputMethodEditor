// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useState } from "react";

export function usePrefersColorScheme() {
  const [isDarkMode, setIsDarkMode] = useState(
    () => window.matchMedia("(prefers-color-scheme: dark)").matches,
  );

  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = () => setIsDarkMode(mediaQuery.matches);

    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, []);

  return isDarkMode;
}

/** Whether the candidate list's background is dark, as `is_dark` in the
 * settings app tells; the settings' windows go dark with it. */
export function isDark(rgba: string): boolean {
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(rgba.slice(i, i + 2), 16));
  return 299 * r + 587 * g + 114 * b < 128000;
}
