# InputMethodEditor

免安裝的 Windows 中文輸入法：解壓縮、註冊一次就能用。
核心是 [libchewing](https://codeberg.org/chewing/libchewing)（新酷音）。

## 功能

* 注音：新酷音的智慧選字、選字修正與自動學習
* 正體／簡體輸出切換，可選擇只轉字形（軟體→软体）或連用語一起轉（軟體→软件）
* 拼音：連打、自動學習（開發中）

## 系統需求

Windows 10／11 x64。在 ARM64 版 Windows 上只能用於 x64／x86 程式。

## 安裝

1. 把 `InputMethodEditor.zip` 解壓到固定的位置，建議
   `C:\Program Files\InputMethodEditor`（原因見下方〈安全性〉）。
   註冊後不要搬移或刪除這個資料夾。
2. 執行 `register.bat`，在 UAC 視窗按「是」。
3. 已開啟的程式要重新開啟才能使用。

Windows 只從 HKLM 讀取輸入法的 COM 註冊，所以註冊需要一次系統管理員權限。
若平常使用標準使用者帳號、UAC 時輸入的是另一個管理員帳號，輸入法只會加到
那個管理員帳號的清單；請回到自己的帳號，從「設定 → 時間與語言 → 語言」
在中文（台灣）底下手動新增 InputMethodEditor。

## 使用

| 按鍵 | 功能 |
|---|---|
| Shift | 切換中文／英文 |
| Ctrl+F12 | 切換正體／簡體輸出 |
| Ctrl+Delete（選字時） | 忘記選中的詞，不再優先出現 |

在工作列輸入法的「中」圖示上按右鍵，可以查網路辭典、切換全形英數、
簡體輸出與大陸用語。正體／簡體設定所有程式共用。
注音的其他操作見右鍵選單的「新酷音使用說明」。

## 移除

先執行 `unregister.bat`，再刪除資料夾。使用者詞庫與設定不會被刪除，
分別在 `%AppData%\InputMethodEditor` 與 `HKEY_CURRENT_USER\Software\InputMethodEditor`，
不需要時請手動刪除。

## 安全性

輸入法是 DLL，會被載入每一個程式，包括以系統管理員身分執行的程式。
資料夾放在 Program Files 以外時，任何以你的身分執行的程式都能替換這個 DLL，
進而取得系統管理員權限。`register.bat` 在這種情況下會先警告。

## 隱私

本程式不會把任何資料傳送到網路上的其他系統，除非使用者或安裝、操作它的人明確要求。

## 授權與來源

GPL-3.0-or-later。

fork 自 [windows-chewing-tsf](https://codeberg.org/chewing/windows-chewing-tsf)
（Kan-Ru Chen 與新酷音貢獻者），以上游 26.9.0（commit `40a665f`）為起點，
上游的完整歷史請見原 repo。
