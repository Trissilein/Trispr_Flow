# Trispr Flow

> GPU-first offline dictation tool with privacy-first local pipeline and optional Claude fallback

[![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat&logo=windows&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![macOS](https://img.shields.io/badge/macOS-000000?style=flat&logo=apple&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Trispr Flow is a modern desktop dictation application built with Tauri, Rust, and TypeScript. It combines the power of local GPU-accelerated transcription (whisper.cpp) with a beautiful, responsive UI for seamless voice-to-text conversion.

## ✨ Features

### Core Functionality
- **🎙️ Push-to-Talk & Toggle Modes** - Record with global hotkeys
- **⚡ GPU-Accelerated Transcription** - Fast local processing with whisper.cpp
- **🔒 Privacy-First** - All processing happens on your machine by default
- **🌐 Multi-Language Support** - English and German with auto-detection
- **☁️ Claude Fallback** - Optional cloud transcription for complex audio

### Phase 1 (Complete) ✅
- **📊 Visual Recording Overlay** - Always-on-top status indicator with states:
  - 🔴 Recording (red pulse)
  - 🟡 Transcribing (yellow spinner)
  - Hidden when idle
- **⌨️ Advanced Hotkey System**
  - Visual hotkey recorder with real-time capture
  - Format validation and conflict detection
  - Inline success/error indicators
  - Both PTT (hold) and Toggle (click) modes in PTT
- **🚨 Comprehensive Error Handling**
  - Categorized error types with recovery suggestions
  - Toast notifications for user feedback
  - Structured logging with tracing
- **🔊 Audio Cues** (Toggleable)
  - Rising beep on recording start
  - Falling beep on recording stop
  - 100ms non-intrusive feedback
- **🌙 Dark Mode** - Professional dark theme with compact UI
  - 40-60% spacing reduction for efficient layout
  - Dark color scheme with high contrast
  - Expander indicators with chevron icons

### Coming Soon

- **🎯 Voice Activity Detection (VAD)** - Automatic silence trimming
- **📝 Text Post-Processing** - Punctuation, number formatting, custom vocabulary
- **📊 Local Analytics Dashboard** - Usage insights (privacy-first)
- **📤 Export Options** - Plain text, Markdown, JSON, CSV

## 🚀 Quick Start

### Prerequisites
- Windows 10/11 or macOS 10.15+
- CUDA-capable GPU (recommended) or CPU fallback
- ~2GB disk space for models

### Installation

#### Build from Source
```bash
# Clone the repository
git clone https://github.com/Trissilein/Trispr_Flow.git
cd Trispr_Flow

# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## 🎮 Usage

### Basic Workflow
1. **Launch** Trispr Flow - app opens in system tray
2. **Configure** hotkeys in the Capture section (default: `Ctrl+Shift+Space`)
3. **Select** your microphone from Input device dropdown
4. **Press and hold** your PTT hotkey while speaking
5. **Release** to transcribe - text is automatically pasted to your active window

### Settings

#### Capture

- **Mode**: Voice Activity Detection (VAD) or Push-to-Talk (PTT)
- **PTT Hotkey (Hold)**: Customizable global shortcut - hold to record
- **Toggle Hotkey (Click)**: Customizable global shortcut - click to start/stop
- **Input Device**: Select your preferred microphone

#### Transcription
- **Model**: Choose between quality (large-v3) or speed (large-v3-turbo)
- **Language**: Auto-detect German/English or specify
- **Claude Fallback**: Optional cloud transcription for complex audio
- **Audio Cues**: Toggle sound feedback on/off with volume control

#### Model Manager
- Download whisper.cpp models on-demand
- Track download progress
- Manage installed models

## 🏗️ Architecture

### Tech Stack
- **Frontend**: TypeScript + Vite + Vanilla JS (no framework overhead)
- **Backend**: Rust + Tauri 2.0
- **Transcription**: whisper.cpp (GPU via CUDA/Metal, CPU fallback)
- **Audio**: cpal for cross-platform audio capture
- **Hotkeys**: tauri-plugin-global-shortcut

### Project Structure
```
Trispr_Flow/
├── src/                    # Frontend TypeScript
│   ├── main.ts            # Main app logic
│   ├── overlay.ts         # Recording overlay
│   └── styles.css         # Application styling
├── src-tauri/             # Rust backend
│   ├── src/
│   │   ├── lib.rs         # Core app logic
│   │   ├── errors.rs      # Error handling
│   │   ├── hotkeys.rs     # Hotkey validation
│   │   └── overlay.rs     # Overlay window management
│   └── Cargo.toml
├── overlay.html           # Overlay UI
├── index.html             # Main window UI
├── ROADMAP.md            # Development roadmap
└── README.md             # This file
```

## 🗺️ Roadmap

### ✅ Milestone 1 - MVP (Complete)
- Basic PTT recording and transcription
- Settings persistence
- History management

### ✅ Milestone 2 - Foundation & Critical UX (Complete)
- Recording overlay with visual states
- Advanced hotkey system with validation
- Error recovery and logging
- Audio cues

### 📋 Milestone 3 - Quality of Life (Next)
- Voice Activity Detection (VAD)
- Text post-processing pipeline
- Multi-language context switching
- Additional keyboard shortcuts
- Undo for paste

### 📋 Milestone 4 - Advanced Features
- Dark mode
- Export options
- Custom model support
- Local analytics dashboard

### 📋 Milestone 5 - Production Ready
- macOS testing and fixes
- Professional installers
- Auto-update mechanism
- Auto-start configuration

See [ROADMAP.md](ROADMAP.md) for detailed implementation plans.

## ⚙️ Configuration

### Environment Variables
- `TRISPR_WHISPER_CLI`: Path to whisper-cli binary
- `TRISPR_WHISPER_MODEL`: Path to ggml model file
- `TRISPR_WHISPER_MODEL_DIR`: Directory containing model files
- `TRISPR_WHISPER_MODEL_BASE_URL`: Base URL for model downloads
- `TRISPR_CLOUD_ENDPOINT`: HTTP endpoint for cloud fallback
- `TRISPR_CLOUD_TOKEN`: Bearer token for cloud authentication

### Local whisper.cpp Setup
```bash
# Build whisper-cli with CUDA support
cd D:\GIT\whisper.cpp
make

# Place models in whisper.cpp/models/
# Example: ggml-large-v3.bin

# Export paths if needed
export TRISPR_WHISPER_CLI=/path/to/whisper-cli
export TRISPR_WHISPER_MODEL_DIR=/path/to/models
```

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

### Development Setup
1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes with conventional commits
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

### Code Style
- **Rust**: Follow `rustfmt` and `clippy` guidelines
- **TypeScript**: ESLint + Prettier
- **Commits**: Conventional Commits format

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [whisper.cpp](https://github.com/ggerganov/whisper.cpp) - Fast whisper inference
- [Tauri](https://tauri.app) - Rust-powered desktop framework
- [Anthropic Claude](https://anthropic.com) - Optional cloud transcription fallback

## 📧 Contact

Project Link: [https://github.com/Trissilein/Trispr_Flow](https://github.com/Trissilein/Trispr_Flow)

---

**Status**: Active Development | **Version**: 0.1.0 | **Phase**: Milestone 2 Complete ✅
