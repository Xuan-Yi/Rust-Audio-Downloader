# Rust Audio Downloader App

This app is a Rust + TypeScript port of the original Python project at `https://github.com/Xuan-Yi/Audio-Downloader.git`.

## App (Rust backend + TypeScript frontend)

Requirements:
- `yt-dlp` on PATH
- `ffmpeg` on PATH (yt-dlp uses it for audio extraction/conversion)
- `pnpm` on PATH
```powershell
scoop install yt-dlp
scoop install ffmpeg
```
Install pnpm globally:
```powershell
npm i -g pnpm
```

Run both (single command):
```powershell
cd .\
pnpm install
pnpm run dev
```
This uses `cargo run`, so the backend is compiled as needed (incremental build).

## Debug/Manual commands

Manual run (backend only):
```powershell
cd .\app\backend
cargo run
```

Manual run (backend check only):
```powershell
cd .\app\backend
cargo check
```

Manual run (frontend only):
```powershell
cd .\app\frontend
pnpm install
pnpm run dev
```

Build frontend:
```powershell
cd .\app\frontend
pnpm run build
```

Open: `http://localhost:5173`

Notes:
- Import accepts `.xlsx`/`.csv` file uploads.
- Export and sample download return files directly from the backend.
- Preview downloads a cached audio file via `yt-dlp` and streams it from `/preview`.
- `npm audit` may report moderate warnings from Vite dependencies.
