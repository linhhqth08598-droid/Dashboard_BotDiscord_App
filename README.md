# Discord Bot Dashboard (ứng dụng Windows + Android)

Cửa sổ desktop (Tauri) mở thẳng dashboard web https://linhhq.io.vn,
nên giao diện, đăng nhập Discord và chọn server giống hệt bản web.

## Cài đặt nhanh trên Windows (qua Terminal)

Mở **PowerShell** và chạy một trong hai lệnh sau để tự động tải và cài đặt phiên bản mới nhất:

- **Cách 1: Cài nhanh 1 dòng lệnh (PowerShell Script)**

  ```powershell
  irm https://githubusercontent.com | iex
  ```

- **Cách 2: Cài qua Windows Package Manager (Winget)**
  ```powershell
  winget install Linhhq.DiscordBotDashboard
  ```

## Chạy

    npm install
    npx tauri dev

## Đóng gói bản cài đặt

    npx tauri build

## Đổi địa chỉ dashboard

Sửa hằng số `DASHBOARD_URL` trong `app-shell/index.html`.

## Bản Android (APK)

Cần: Android Studio (SDK + NDK), JDK 17, và các biến môi trường `JAVA_HOME`, `ANDROID_HOME`, `NDK_HOME`.

    rustup target add aarch64-linux-android armv7-linux-androideabi
    npx tauri android init          # chỉ chạy 1 lần, tạo src-tauri/gen/android
    npx tauri android build --apk --debug --target aarch64   # APK thử nghiệm (tự ký debug)

APK thử nằm ở `src-tauri/gen/android/app/build/outputs/apk/`. Bản phát hành chính thức do GitHub Actions
build và ký khi push tag `v*` (cần 4 secret `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`,
`ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`). Bản Android không tự cài được bản mới, nhưng mỗi lần mở app sẽ đọc `latest.json` trên GitHub:
nếu có bản mới hơn thì hiện hộp thoại **Cập nhật** / **Để sau**. Bấm _Cập nhật_ sẽ mở trang tải
(Release mới nhất) để tải APK và cài đè; bấm _Để sau_ thì lần mở app sau sẽ hỏi lại.
Khi phát hành, nhớ nâng `version` ở `package.json`, `src-tauri/tauri.conf.json` và `src-tauri/Cargo.toml`
cho khớp tag, vì app so sánh phiên bản đang chạy với `latest.json`.
