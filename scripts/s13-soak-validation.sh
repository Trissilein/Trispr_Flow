#!/bin/bash

# S13 Manual Soak Gate Validation Script
# Abbreviated soak test: 10 overlay toggles, 3 restarts, module toggles
# Full soak (50 cycles + 10 restarts) should be run manually for extended stability proof

set -e

cd "$(dirname "$0")/.."

echo "=== S13 Soak Validation ==="
echo ""

# Pre-flight checks
echo "[1/4] Pre-flight checks..."
if ! command -v npm &> /dev/null; then
    echo "ERROR: npm not found"
    exit 1
fi

if ! command -v cargo &> /dev/null; then
    echo "ERROR: cargo not found"
    exit 1
fi

echo "✓ npm and cargo available"
echo ""

# Automated gates
echo "[2/4] Running automated gates..."
echo ""

echo "  → npm run build"
npm run build > /dev/null 2>&1 && echo "  ✓ npm run build passed" || {
    echo "  ✗ npm run build failed"
    exit 1
}

echo "  → npm test"
npm test > /dev/null 2>&1 && echo "  ✓ npm test passed" || {
    echo "  ✗ npm test failed"
    exit 1
}

echo "  → cargo test --lib"
cd src-tauri && cargo test --lib > /dev/null 2>&1 && echo "  ✓ cargo test --lib passed" || {
    echo "  ✗ cargo test --lib failed"
    exit 1
}
cd ..

echo ""
echo "[3/4] Manual soak steps (abbreviated - full soak run manually):"
echo ""
echo "  Overlay stress test (10 toggles):"
echo "    1. Start app: npm run tauri -- dev --no-watch"
echo "    2. Click overlay button 10 times (show/hide cycle)"
echo "    3. During overlay activity:"
echo "       - Start transcription with microphone"
echo "       - Toggle ai_refinement module on/off"
echo "       - Toggle output_voice_tts module on/off"
echo "    4. Check overlay remains visible after recovery"
echo "    5. Check refinement pulse resyncs after module toggle"
echo ""
echo "  App restart stress test (3 restarts):"
echo "    1. Restart app 3 times"
echo "    2. After each restart, verify:"
echo "       - Module state is restored from settings"
echo "       - Overlay visual state is recovered"
echo "       - No permanent lockout after overlay failure"
echo ""
echo "  Expected results:"
echo "    ✓ No lost overlay visibility after recovery"
echo "    ✓ Refinement pulse animated and resynced after restart"
echo "    ✓ No hidden background activity when module disabled"
echo "    ✓ Active-tab fallback works (ai_refinement → transcription)"
echo ""
echo "[4/4] Documentation:"
echo ""
echo "  S13 Acceptance Criteria (from TASK_SCHEDULE.md):"
echo "    - Automated gates stay green (verified above)"
echo "    - Manual soak gate (50 cycles + 10 restarts) passes without"
echo "      visibility/pulse regressions"
echo ""
echo "  Full Soak Procedure:"
echo "    1. Run this script (automated gates)"
echo "    2. Perform manual overlay/restart cycles per instructions above"
echo "    3. Document any failures or flaky behavior"
echo "    4. If all pass: update TASK_SCHEDULE.md S13 status to VERIFIED"
echo ""
echo "=== S13 Validation Complete ==="
echo ""
echo "Automated gates are green. Manual soak steps listed above."
echo "See TASK_SCHEDULE.md for full acceptance criteria."
