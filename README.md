# tab-o-txt

Plain text spreadsheet parser & CLI editor.\
用于纯文本表格的解析器，以及命令行界面编辑器。

## Usage 用法
Use the CLI editor:\
使用命令行界面编辑器：
```sh
tab-o-txt [file-name]
```
Use the parser:\
使用解析器：
```rust
use tab_o_txt::sheet::Sheet;

fn main() {
    let txt = "This\tis\tan
\texample\tof\ta\tplain-text
\t\t\t\tspreadsheet";
    let sheet = Sheet::from_str(txt);

    assert_eq!("example", sheet.content_at((1, 1)).unwrap());
}
```
