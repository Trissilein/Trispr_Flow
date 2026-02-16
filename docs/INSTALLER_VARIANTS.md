# Trispr Flow - Installer Variants

Trispr Flow is available in two installer editions to suit different user needs and system configurations.

## 📦 Available Editions

### 1. CUDA Edition (Full)
**Filename:** `Trispr-Flow-v0.4.0-CUDA-Setup.exe`
**Size:** ~700 MB
**GPU Backends:** NVIDIA CUDA + Vulkan

**Includes:**
- ✅ NVIDIA CUDA backend with full runtime libraries
  - `cublas64_13.dll` (50 MB)
  - `cublasLt64_13.dll` (459 MB)
  - `cudart64_13.dll` (460 KB)
- ✅ Vulkan backend (cross-vendor GPU support)
- ✅ CPU fallback

**Recommended for:**
- Users with NVIDIA GPUs who want maximum performance
- Systems where installing CUDA Toolkit separately is not desired
- Users who want the option to choose between CUDA and Vulkan at install time

**Requirements:**
- NVIDIA GPU (GTX 900 series or newer recommended)
- Recent NVIDIA drivers (Game Ready or Studio)
- Windows 10/11 x64

---

### 2. Vulkan Edition (Lite)
**Filename:** `Trispr-Flow-v0.4.0-Vulkan-Setup.exe`
**Size:** ~200 MB
**GPU Backends:** Vulkan only

**Includes:**
- ✅ Vulkan backend (works on NVIDIA, AMD, and Intel GPUs)
- ✅ CPU fallback
- ❌ No CUDA runtime libraries (smaller download)

**Recommended for:**
- Users with AMD or Intel GPUs
- Users who prefer smaller installer size
- NVIDIA users who don't mind slightly reduced performance (~10-15% slower than CUDA)
- Systems with limited disk space

**Requirements:**
- Any GPU with Vulkan 1.2+ support (most GPUs from 2016+)
- Updated graphics drivers
- Windows 10/11 x64

---

## 🔧 Building the Installers

### Build Both Variants
```bash
# From project root
build-both-installers.bat
```

This script will:
1. Clean previous builds
2. Build the frontend (once, shared by both)
3. Build CUDA edition → `Trispr-Flow-v0.4.0-CUDA-Setup.exe`
4. Build Vulkan edition → `Trispr-Flow-v0.4.0-Vulkan-Setup.exe`

### Build Individual Variants

**CUDA Edition only:**
```bash
npm run build
cd src-tauri
cargo tauri build --config tauri.conf.json
```

**Vulkan Edition only:**
```bash
npm run build
cd src-tauri
cargo tauri build --config tauri.conf.vulkan.json
```

---

## 📊 Performance Comparison

| Edition | NVIDIA GPU | AMD GPU | Intel GPU | CPU Only |
|---------|-----------|---------|-----------|----------|
| **CUDA** | ⚡⚡⚡ Fastest | ❌ N/A | ❌ N/A | ✅ Supported |
| **Vulkan** | ⚡⚡ Fast | ⚡⚡ Fast | ⚡ Moderate | ✅ Supported |

**Notes:**
- CUDA on NVIDIA GPUs: ~10-15% faster than Vulkan for Whisper inference
- Vulkan performance varies by driver quality (NVIDIA > AMD > Intel)
- CPU fallback performance is identical across both editions

---

## 🛠️ Technical Details

### CUDA Edition Configuration
- **Config:** `src-tauri/tauri.conf.json`
- **Installer Hooks:** `src-tauri/nsis/hooks.nsh`
- **Bundled Resources:**
  - `bin/cuda/*` (Whisper binaries + CUDA runtime)
  - `bin/vulkan/*` (Vulkan alternative)
  - User selects backend during installation
  - Unused backend is deleted post-install

### Vulkan Edition Configuration
- **Config:** `src-tauri/tauri.conf.vulkan.json`
- **Installer Hooks:** `src-tauri/nsis/hooks.vulkan.nsh`
- **Bundled Resources:**
  - `bin/vulkan/*` only
  - No GPU backend selection page (Vulkan is hardcoded)

---

## ⚖️ Legal & Licensing

### CUDA Runtime Redistribution
The CUDA edition includes NVIDIA CUDA runtime libraries redistributed under NVIDIA's EULA:
- `cublas64_13.dll`
- `cublasLt64_13.dll`
- `cudart64_13.dll`

**Redistribution is permitted** subject to:
- ✅ Bundling with application software (not standalone)
- ✅ NVIDIA copyright notices preserved
- ✅ No modification of library files

**Reference:** NVIDIA CUDA Toolkit End User License Agreement (EULA), Section 2.6 - Distribution

### Whisper.cpp
Both editions use whisper.cpp (MIT License) compiled with CUDA/Vulkan support.

---

## 🐛 Troubleshooting

### CUDA Edition Issues

**Error: "cublas64_13.dll not found"**
- **Cause:** CUDA runtime not properly bundled
- **Solution:** Reinstall using the CUDA edition installer, or manually copy DLLs to `bin/cuda/`

**CUDA backend not working despite NVIDIA GPU**
- Check NVIDIA driver version (minimum: 525.60.11 for CUDA 12.x)
- Try selecting Vulkan during installation as fallback

### Vulkan Edition Issues

**Error: "Failed to initialize Vulkan"**
- Update graphics drivers (NVIDIA/AMD/Intel)
- Check GPU supports Vulkan 1.2+ (run `vulkaninfo` in cmd)
- Application will auto-fallback to CPU if Vulkan fails

**Slow performance on NVIDIA GPU**
- Consider reinstalling with CUDA edition for +10-15% speed
- Ensure GPU is not in power-saving mode

---

## 📝 Changelog

### v0.4.0 - Current
- ✅ Introduced dual installer variants (CUDA + Vulkan)
- ✅ Bundled full CUDA runtime libraries in CUDA edition
- ✅ Optimized Vulkan edition for smaller download size
- ✅ Installer size reduction: Vulkan edition 65% smaller

---

## 🤝 Contributing

When submitting builds:
1. Always build both variants before release
2. Test CUDA edition on NVIDIA hardware
3. Test Vulkan edition on AMD/Intel hardware
4. Verify installer size matches expected values
5. Check that post-install GPU backend cleanup works correctly

**Build checklist:**
- [ ] Run `build-both-installers.bat`
- [ ] Verify CUDA installer includes all 3 CUDA DLLs
- [ ] Verify Vulkan installer excludes CUDA DLLs
- [ ] Test both installers on clean VM/system
- [ ] Check installer names and version numbers
- [ ] Upload both variants to release page

---

For more information, see:
- [Development Guide](DEVELOPMENT.md)
- [Build Documentation](../rebuild-installer.bat)
- [Whisper.cpp Setup](../scripts/setup-whisper.ps1)
