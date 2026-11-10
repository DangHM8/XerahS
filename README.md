<p align="center">
  <img src="assets/logo.png" width="128" alt="XerahS logo">
</p>

<h1 align="center">XerahS</h1>

<p align="center">Công cụ chụp màn hình theo vùng định sẵn, siêu nhanh, cho Windows — viết bằng Rust.</p>

---

## Giới thiệu

XerahS chạy nền dưới khay hệ thống. Bạn tạo các **preset** — mỗi preset là một vùng màn hình cố định gắn với một phím tắt toàn cục. Nhấn phím tắt là vùng đó được chụp ngay, lưu thành file theo mẫu tên bạn đặt, và hiện một thông báo (toast) kiểu ShareX ở góc màn hình.

Phù hợp khi bạn cần chụp lặp đi lặp lại cùng một khu vực (bảng số liệu, cửa sổ game, khung chat, dashboard…) mà không phải kéo chọn mỗi lần.

## Tính năng

- **Preset vùng chụp + phím tắt toàn cục** (vd. `Ctrl+Shift+1`, `Alt+F2`), hoạt động cả khi XerahS không ở tiền cảnh.
- **Chọn vùng trực quan** bằng overlay kéo thả trên màn hình.
- **Đa màn hình & DPI hỗn hợp**: dùng tọa độ pixel vật lý, hỗ trợ màn hình nằm bên trái/trên màn hình chính.
- **PNG hoặc JPG**, chỉnh chất lượng; thư mục lưu, định dạng và mẫu tên có thể đặt riêng cho từng preset.
- **Mẫu tên file** linh hoạt (xem bên dưới).
- **Toast tùy biến**: góc hiển thị, kích thước, thời gian, hiệu ứng mờ, ảnh thu nhỏ, và hành động cho chuột trái/phải/giữa: mở file, mở thư mục, sao chép đường dẫn, xóa file.
- **Cửa sổ cài đặt** (egui) mở từ menu khay hệ thống, có kiểm tra phím tắt hợp lệ.
- Nhẹ, một file `.exe` duy nhất, không cần cài đặt.

## Cài đặt & build

Yêu cầu: Windows 10/11, [Rust](https://rustup.rs) (toolchain MSVC).

```bash
git clone https://github.com/DangHM8/XerahS.git
```

```bash
cd XerahS
```

```bash
cargo build --release
```

File chạy: `target/release/xerahs.exe`. Mở lên, biểu tượng XerahS sẽ xuất hiện ở khay hệ thống.

## Sử dụng

1. Nhấp chuột phải vào biểu tượng khay → **Settings**.
2. Thêm preset, chọn vùng chụp, đặt phím tắt.
3. Lưu cài đặt, rồi nhấn phím tắt để chụp.
4. Nhấp vào toast để mở ảnh / thư mục (tùy cấu hình).

Thoát: chuột phải biểu tượng khay → **Quit**.

### Mẫu tên file

Mặc định: `XerahS_%Y-%m-%d_%H-%M-%S_%n`

| Token | Ý nghĩa |
|---|---|
| `%Y` `%m` `%d` | Năm, tháng, ngày |
| `%H` `%M` `%S` | Giờ, phút, giây |
| `%n` | Bộ đếm tăng dần (3 chữ số) |
| `%name` | Tên preset |
| `%w` `%h` | Chiều rộng / cao ảnh |
| `%title` | Tiêu đề cửa sổ đang hoạt động |

Phần mở rộng (`.png` / `.jpg`) được thêm tự động. Ký tự không hợp lệ trong tên file Windows sẽ được loại bỏ.

## Đường dẫn

| Mục | Vị trí |
|---|---|
| Cài đặt | `%APPDATA%\XerahS\settings.json` |
| Ảnh chụp (mặc định) | `%USERPROFILE%\Pictures\XerahS` |
| Log chẩn đoán | `%TEMP%\xerahs.log` |

## Cấu trúc mã nguồn

```
src/
  main.rs       vòng lặp chính, xử lý phím tắt và chụp
  capture.rs    chụp màn hình (GDI BitBlt)
  overlay.rs    overlay chọn vùng
  hotkey.rs     đăng ký phím tắt toàn cục
  save.rs       mã hóa & lưu PNG/JPG
  naming.rs     mẫu tên file
  toast.rs      thông báo sau khi chụp
  tray.rs       biểu tượng & menu khay hệ thống
  config_ui.rs  cửa sổ cài đặt (egui)
  settings.rs   đọc/ghi settings.json
  model.rs      kiểu dữ liệu dùng chung
assets/         logo & icon
```

> ⚠️ Hành động **DeleteFile** trên toast xóa file vĩnh viễn, không hỏi lại.
