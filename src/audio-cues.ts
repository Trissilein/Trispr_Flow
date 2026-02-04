// Audio cue playback using Web Audio API
import { settings } from "./state";

let audioContext: AudioContext | null = null;

export function playAudioCue(type: "start" | "stop") {
  try {
    // Initialize AudioContext lazily (requires user interaction first)
    if (!audioContext) {
      audioContext = new AudioContext();
    }

    const now = audioContext.currentTime;
    const oscillator = audioContext.createOscillator();
    const gainNode = audioContext.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(audioContext.destination);

    // Different frequencies for start and stop
    if (type === "start") {
      // Rising beep: 600Hz -> 800Hz
      oscillator.frequency.setValueAtTime(600, now);
      oscillator.frequency.linearRampToValueAtTime(800, now + 0.1);
    } else {
      // Falling beep: 800Hz -> 600Hz
      oscillator.frequency.setValueAtTime(800, now);
      oscillator.frequency.linearRampToValueAtTime(600, now + 0.1);
    }

    // Quick fade in/out
    const volume = settings?.audio_cues_volume ?? 0.3;
    const target = Math.max(0, Math.min(1, volume));
    gainNode.gain.setValueAtTime(0, now);
    gainNode.gain.linearRampToValueAtTime(target, now + 0.01);
    gainNode.gain.linearRampToValueAtTime(0, now + 0.1);

    oscillator.start(now);
    oscillator.stop(now + 0.1);
  } catch (error) {
    console.error("Failed to play audio cue:", error);
  }
}
