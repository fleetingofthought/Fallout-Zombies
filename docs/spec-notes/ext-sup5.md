# SUP NVSE batch 5: the files and the machine (2026-11-08, #1008)

42 SUP functions: the Excel workbooks (13), the JSON files (2), file sizes and times and
the load-order dump (4), screenshots and QR codes (5), and the functions of the network,
the desktop, the clipboard and the computer's fonts and language (18), which give their
pages' nothing values by the owner's decisions of 2026-11-08. Code:
`crates/fnv-game/src/nvse_sup5.rs` (the functions), `crates/fnv-game/src/xls.rs` (the
workbook format), `crates/fnv-game/src/image_files.rs` (the picture files),
`crates/fnv-game/src/mod_files.rs` (binary files in NVForever's layer) and the engine's
side in `engine/nvf-play/src/sup_shots.rs`. Tests: `crates/fnv-game/tests/sup5.rs` and the
unit tests of the three files.

Source: the GECK wiki export [documented: the page named]; the workbook format from
Microsoft's open specifications [documented: MS-CFB, MS-XLS]. Not probed yet: set `sup5`
of `tools/capture/jip_probes.py` (lab `L5.ext.sup5`) asks the questions below; until it
runs, every value a page leaves open is a [hypothesis].

Every file is under the game's folder unless said otherwise, read through
`ModFiles::read_root` (this session's write, then NVForever's copy in its layer's
`GameRoot` folder, then the game folder's) and written only into NVForever's layer, never
the game folder (CLAUDE.md charter). A path that could leave the folder (a root, a drive,
`.` or `..`) is not used [decision: the charter, as `mod_files::on_disk`].

## 1. Excel workbooks

| Function | NVForever |
|---|---|
| ExcelWriteFloat, ExcelWriteString `file sheet row col value` | 1 written; the file and the sheet made when missing; a file that is no workbook -1 and left as it is [documented]. `.xls` added to the name [documented: "File name is specified without extension"]. A number is written as a float, type 2, a whole one too [hypothesis]. Rows past 65,535 and columns past 255 (the format's limits) -1 [hypothesis] |
| ExcelReadFloat, ExcelReadString `file sheet row col` | the number, the text [documented]; a missing file -1 [documented]; ReadString of a missing file "", an empty cell 0 and "", a text cell's number 0, a number cell's text "" [hypothesis] |
| ExcelGetCellValueType | 0 empty, 1 int, 2 float, 3 text, 4 wide text, 5 formula [documented: the page's table]; text stored 16 bits a character is 4, text NVForever writes is 3 unless a character is past U+00FF [hypothesis] |
| ExcelGetWorkSheetCount `file`; ExcelGetRowCount, ExcelGetColCount `file sheet` | the sheets; the last used row and column + 1, 0 for an empty sheet [hypothesis: the page says "number of rows"]; -1 for a missing file [documented] or sheet [hypothesis] |
| ExcelEraseCell, ExcelEraseSheet | the cell emptied; every cell of the sheet emptied, the sheet kept [documented: "Will erase all cells in Sheet"]; 1, or -1 [hypothesis for a missing sheet] |
| ExcelSetSheetName `file sheet [name]` | renames (cut to 31 characters [documented]) and returns the name; without a name returns it [documented]; "" for a missing file or sheet [hypothesis: the page's -1 is a number, the function returns text] |
| ExcelArrayToFile `file sheet row col array [horizontal]` | the values down a column, along a row with 1; a form as its name [documented: PlayerRef written as "Courier"]; nested arrays passed over [hypothesis] |
| ExcelArrayFromFile `file sheet n readRow` | column n's values (row n's with 1), `*Plugin.esm:hex` as that plugin's form [documented]; empty cells passed over, an unknown plugin or form kept as text, a missing file no array [hypothesis: the page's -1 cannot be an array] |
| aliases `XLS…` | the catalogue's [documented] |

A sheet past the last one is made with the ones between, named `Sheet1`, `Sheet2`, …
[hypothesis]. Sheets are numbered from 0, rows and columns from 0 [documented].

**The file** (`crate::xls`): a compound file [documented: MS-CFB, the header, the sector
allocation table and its DIFAT sectors, the directory, the mini stream] holding a BIFF8
workbook stream `Workbook` (`Book` for BIFF5) [documented: MS-XLS]. Read: the sheets
(BOUNDSHEET), the shared strings (SST, with the CONTINUE rule for split characters) and
the cells: NUMBER, RK, MULRK, LABELSST, LABEL, BOOLERR (a boolean as 1 or 0), FORMULA with
its number, boolean or string (STRING) result. Charts and macro sheets are passed over.
Written: one stream with CODEPAGE 1200, the font, style and cell XF records a workbook
starts with, the sheets' names, the shared strings and per sheet DIMENSIONS, WINDOW2 and
the cells (whole numbers in 30 bits as RK, others as NUMBER, text as LABELSST). The stream
is padded to 4,096 bytes so the file needs no mini stream [design]. A formula read from a
file is written back as its last result [decision: NVForever writes no formulas].
Formatting is neither read nor kept. Not checked against Excel on this machine (none
installed): the reader reads what the writer writes, including a string table long
enough for CONTINUE records (unit tests).

## 2. JSON files

| Function | NVForever |
|---|---|
| JSONIsFileValid `file` | 1 valid, -3 not valid JSON, -4 missing, -5 outside the game folder, -6 not `.json` [documented: the page's table]; a UTF-8 byte order mark passed over |
| JSONGetValue `file key cached` | the key's value as an array: an object as a string map, a list as a list, any other value (number, text, true/false as 1/0, null as 0) in a one-element list; the key whole, else a path split at `.` or `/` with list indexes; no array for a missing key or file [hypothesis: the article the page points to is not in the export]. `cached` keeps the parsed file for later cached calls of the session [hypothesis: the parameter's name] |

## 3. File sizes and times, the load-order dump

| Function | NVForever |
|---|---|
| GetFileSize `file` | the size in bytes [documented]; -1 missing [hypothesis] |
| GetFileTime `file request` | 0 created, 1 last read, 2 last written [documented], as a list year, month, day, hour, minute, second in local time [hypothesis: the page shows no element]; an empty list for a missing file or another request [hypothesis] |
| GetFileTimeSTR `file request type` | `DD/MM/YY HH:MM`, `MM/DD/YY HH:MM` with type 1 [documented]; "" with no time [hypothesis] |
| DumpLoadOrder (DumpLO) | the load order into `SUPNVSE.log` in the game folder [documented], NVForever's copy, after the lines this session wrote: one line a plugin, the index in hex and the name (`00 FalloutNV.esm`) [hypothesis: the page shows no line] |

A file written this session only (no write root: a headless run) has its size but no
times.

## 4. Screenshots and QR codes

| Function | NVForever |
|---|---|
| CaptureScreenshot (Screenshot) `[file] [format]` | the window into the game folder, `a_Screenshot.jpg` by default, the extension by the format 0 JPG, 1 BMP, 2 GIF, 3 TIFF, 4 PNG, 5 DDS [documented] |
| CaptureScreenshotAlt `folder file x0 x1 y0 y1 format gameRes quality [elements]` | the part of the window into `Data\textures\SUPScreenshots\<folder>\<file>.<ext>`, format 6 a smaller DDS; the coordinates in pixels with Use Game Res, else in the UI's units, 960 high [documented: the page; the tile tree's screen()]. A part outside the window writes nothing |
| DeleteScreenshot `folder file` | removes `Data\textures\SUPScreenshots\<folder>\<file>` (the extension given): -1 no such file, -2 a folder outside [documented]; 1 removed [hypothesis]. Only files of NVForever's layer: the game's own Data is never changed |
| TextToQRCode `file text [convert]` | a PNG of the text's QR code, `<file>.png`, its folders made [documented]; under the game folder, black on white, 10 pixels a module with a quiet zone of 4 modules, error correction M [hypothesis: the page gives none of these]. The text is written as UTF-8 either way: NVForever's strings are Unicode already |
| DrawGraphicsToDDSTextureAndSave | writes nothing [decision: drawing text in the system's fonts and pictures into a texture is a design of its own; follow-up] |

**The engine** (`nvf_play::sup_shots`): the requests (`Game::ui.screenshots`,
`image_files::ShotRequest`) are taken after the frame's scripts; each is a picture of the
window (Bevy's `Screenshot::primary_window`), cut, encoded by `image_files::encode` and
written through `ModFiles::write_root_bytes` / `write_bytes`. Logged:
`sup: screenshot <path> <w>x<h>`. Encoded: PNG (the `png` crate), BMP (24-bit) and DDS
(uncompressed 32-bit BGRA; format 6 16-bit R5G6B5 [hypothesis: "smaller size"]); the
original's DDS is "not real DDS" [documented], NVForever's is. JPG, GIF and TIFF have no
encoder in NVForever: they are written as PNG data under the name asked for [decision: no
image encoder added for screenshots alone, the 2026-11-08 brief]. CaptureScreenshotAlt's
quality and drawn elements are not used [decision: the elements' text needs the system's
fonts]. The picture includes the HUD and menus as drawn. Not checked by a smoke scenario
yet.

Follow-ups: JPG/GIF/TIFF encoders; the drawn elements and DrawGraphicsToDDSTextureAndSave
(text in a font, pictures placed on a texture).

## 5. The nothing values (owner decisions of 2026-11-08)

| Function | NVForever |
|---|---|
| GetNexusModVersion, GetNexusModVersionAPI, GetGitHubLatestReleaseTag | "ERROR", the pages' answer when the internet cannot be reached [documented; decision: no network] |
| GetNexusModVersionMulti `ids` | a list of "ERROR", one a mod [documented: "will return ERROR"; decision] |
| GetNVSEOnlineVersion, IsNexusAPIKeyValid | 0 [decision] |
| GetPastebinRawTextString, GetPastebinRawTextArray | "", an empty list [decision] |
| OpenNexusModPage, OpenNexusUserPage, OpenFolder | nothing opened [decision: no programs, folders or pages] |
| IsProgramRunning | 0 [decision] |
| OpenFileDialogBox, OpenFolderDialogBox `udf …` | no dialog; the handler is called next frame as when the player closes the dialog without a choice, iSuccess 0 and an empty path [documented: the handler's "iSuccess will return 0 if file is not chosen by user"; decision: no dialogs]; returns "" |
| StringToClipboard, ClipboardToString | nothing kept, "" [decision: no channel to the clipboard: the engine leaves Bevy's ClipboardPlugin out (nvforever main.rs); follow-up] |
| GetWindowsFonts, GetUserWindowsLanguage | an empty list, "" [decision: the engine has no list of the system's fonts and no language of the system; follow-up] |

## 6. Probe set `sup5` (not run yet)

Writes under the game folder's `NVFProbe\sup5` (the workbooks, a JSON file through
ppNVSE's pWriteFile, the QR code), the game folder's `SUPNVSE.log` and one screenshot in
`Data\textures\SUPScreenshots\NVFProbe`, which its rows delete again. After the run the
game folder's `NVFProbe` folder and `SUPNVSE.log` are deleted by hand. The network and
dialog functions are not probed (a dialog would halt the game). The questions: a written
number's type (2 for 10?); text across kinds; row and column counts; the sheets made past
the last and their names; a missing sheet's answers; SetSheetName's return; erase's
returns; a form in ExcelArrayToFile; empty cells in ExcelArrayFromFile; JSONGetValue's
array for a value, a list, an object, a dotted path and a missing key; GetFileSize of a
missing file; GetFileTime's elements; DumpLoadOrder's lines; DeleteScreenshot's success
value; where TextToQRCode writes and how large.
