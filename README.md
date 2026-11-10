# Discord Bot Dashboard (ứng dụng desktop)

Cửa sổ desktop (Tauri) mở thẳng dashboard web https://dashboard-botdiscord.onrender.com,
nên giao diện, đăng nhập Discord và chọn server giống hệt bản web.

## Chạy
    npm install
    npx tauri dev

## Đóng gói bản cài đặt
    npx tauri build

## Đổi địa chỉ dashboard
Sửa hằng số `DASHBOARD_URL` trong `app-shell/index.html`.
