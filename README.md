# InputMethodEditor

個人使用的 Windows 中文輸入法，fork 自
[windows-chewing-tsf](https://codeberg.org/chewing/windows-chewing-tsf)
（Kan-Ru Chen 與新酷音貢獻者），
引擎是 [libchewing](https://codeberg.org/chewing/libchewing)（新酷音）。
本 repo 以上游 26.9.0（commit `40a665f`）為起點，上游的完整歷史請見原 repo。

目標：

* 注音：新酷音的智慧選字、選字修正與自動學習
* 拼音：連打、自動學習（尚未實作）
* 輸出正體／簡體切換，可選擇是否轉換用語（尚未實作）

目前是免安裝版的新酷音注音輸入法。

All parts are licensed under GPL-3.0-or-later license.

# 安裝（免安裝版）

1. 把 `InputMethodEditor.zip` 解壓到固定的位置，建議
   `C:\Program Files\InputMethodEditor`（原因見下方〈安全性〉）。
   註冊後不要搬移或刪除這個資料夾。
2. 執行 `register.bat`，在 UAC 視窗按「是」。
3. 已開啟的程式要重新開啟才能使用。

移除：先執行 `unregister.bat`，再刪除資料夾。使用者詞庫與設定不會被刪除，
分別在 `%AppData%\InputMethodEditor` 與 `HKEY_CURRENT_USER\Software\InputMethodEditor`，
不需要時請手動刪除。

Windows 只從 HKLM 讀取輸入法的 COM 註冊，所以註冊需要一次系統管理員權限。
若平常使用標準使用者帳號、UAC 時輸入的是另一個管理員帳號，輸入法只會加到
那個管理員帳號的清單；請回到自己的帳號，從「設定 → 時間與語言 → 語言」
在中文（台灣）底下手動新增 InputMethodEditor。

## 安全性

輸入法是 DLL，會被載入每一個程式，包括以系統管理員身分執行的程式。
資料夾放在 Program Files 以外時，任何以你的身分執行的程式都能替換這個 DLL，
進而取得系統管理員權限。`register.bat` 在這種情況下會先警告。

# 建置

需要 [Rust](https://rustup.rs/)，以及下列其中一種工具鏈。

**Windows，MSVC**

* [Build Tools for Visual Studio](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)

```
rustup target add x86_64-pc-windows-msvc i686-pc-windows-msvc
cargo xtask dist --release
```

**macOS／Linux 交叉編譯，llvm-mingw**

* [llvm-mingw](https://github.com/mstorsjo/llvm-mingw)，把它的 `bin` 加進 `PATH`

```
rustup target add x86_64-pc-windows-gnullvm i686-pc-windows-gnullvm
cargo xtask dist --target gnullvm --release
```

產出 `dist/InputMethodEditor/` 與 `dist/InputMethodEditor.zip`。
辭典從 libchewing-data 下載並以 SHA-256 驗證，快取在 `.cache/`。

## TSF References

* [Text Services Framework](http://msdn.microsoft.com/en-us/library/windows/desktop/ms629032%28v=vs.85%29.aspx)
* [Guidelines and checklist for IME development (Windows Store apps)](http://msdn.microsoft.com/en-us/library/windows/apps/hh967425.aspx)
* [Input Method Editors (Windows Store apps)](http://msdn.microsoft.com/en-us/library/windows/apps/hh967426.aspx)
* [Third-party input method editors](http://msdn.microsoft.com/en-us/library/windows/desktop/hh848069%28v=vs.85%29.aspx)
* [Strategies for App Communication between Windows 8 UI and Windows 8 Desktop](http://software.intel.com/en-us/articles/strategies-for-app-communication-between-windows-8-ui-and-windows-8-desktop)
* [TSF Aware, Dictation, Windows Speech Recognition, and Text Services Framework. (blog)](http://blogs.msdn.com/b/tsfaware/?Redirected=true)
* [Win32 and COM for Windows Store apps](http://msdn.microsoft.com/en-us/library/windows/apps/br205757.aspx)
* [Input Method Editor (IME) sample supporting Windows 8](http://code.msdn.microsoft.com/windowsdesktop/Input-Method-Editor-IME-b1610980)

## Windows ACL (Access Control List) references

* [The Windows Access Control Model Part 1](http://www.codeproject.com/Articles/10042/The-Windows-Access-Control-Model-Part-1#SID)
* [The Windows Access Control Model: Part 2](http://www.codeproject.com/Articles/10200/The-Windows-Access-Control-Model-Part-2#SidFun)
* [Windows 8 App Container Security Notes - Part 1](http://recxltd.blogspot.tw/2012/03/windows-8-app-container-security-notes.html)
* [How AccessCheck Works](http://msdn.microsoft.com/en-us/library/windows/apps/aa446683.aspx)
* [GetAppContainerNamedObjectPath function (enable accessing object outside app containers using ACL)](http://msdn.microsoft.com/en-us/library/windows/desktop/hh448493)
* [Creating a DACL](http://msdn.microsoft.com/en-us/library/windows/apps/ms717798.aspx)

# Privacy Policy

This program will not transfer any information to other networked systems unless
specifically requested by the user or the person installing or operating it.
