// Chapter UI management
// Handles chapter generation, rendering, and navigation

import {
  type Chapter,
  generateSilenceBasedChapters,
  generateTimeBasedChapters,
  generateHybridChapters,
  buildConversationHistory,
} from "./history";
import {
  chaptersContainer,
  chaptersList,
  chapterMethodSelect,
  chaptersToggle,
  historyList,
} from "./dom-refs";

type ChapterMethod = "silence" | "time" | "hybrid";

let currentChapters: Chapter[] = [];
let activeChapterId: string | null = null;
let isChaptersVisible = false;

/**
 * Initialize chapter UI and event listeners
 */
export function initChaptersUI(): void {
  if (!chapterMethodSelect || !chaptersToggle || !chaptersContainer) {
    console.warn("[Chapters] Missing DOM elements, skipping initialization");
    return;
  }

  // Method selector change
  chapterMethodSelect.addEventListener("change", () => {
    regenerateChapters();
  });

  // Toggle button
  chaptersToggle.addEventListener("click", () => {
    toggleChaptersVisibility();
  });

  // Initial generation
  regenerateChapters();
}

/**
 * Generate chapters based on current method selection
 */
function regenerateChapters(): void {
  const method = (chapterMethodSelect?.value || "silence") as ChapterMethod;
  const entries = buildConversationHistory();

  if (entries.length === 0) {
    currentChapters = [];
    renderChapters();
    return;
  }

  switch (method) {
    case "silence":
      currentChapters = generateSilenceBasedChapters(entries, 2000);
      break;
    case "time":
      currentChapters = generateTimeBasedChapters(entries, 5);
      break;
    case "hybrid":
      currentChapters = generateHybridChapters(entries, 2000, 10 * 60 * 1000);
      break;
    default:
      currentChapters = generateSilenceBasedChapters(entries, 2000);
  }

  renderChapters();
}

/**
 * Render chapter list in the UI
 */
function renderChapters(): void {
  if (!chaptersList || !chaptersContainer) return;

  // If no chapters, hide the container
  if (currentChapters.length === 0) {
    chaptersContainer.style.display = "none";
    isChaptersVisible = false;
    return;
  }

  // Show container if hidden
  if (!isChaptersVisible) {
    chaptersContainer.style.display = "block";
    isChaptersVisible = true;
  }

  // Clear existing chapters
  chaptersList.innerHTML = "";

  // Render each chapter
  currentChapters.forEach((chapter) => {
    const item = document.createElement("div");
    item.className = "chapter-item";
    item.dataset.chapterId = chapter.id;
    item.dataset.timestampMs = chapter.timestamp_ms.toString();

    if (chapter.id === activeChapterId) {
      item.classList.add("active");
    }

    // Format timestamp
    const time = formatTimestamp(chapter.timestamp_ms);
    const entryCount = chapter.entry_count;

    item.innerHTML = `
      <div class="chapter-number">Ch ${chapter.label.replace("Chapter ", "")}</div>
      <div class="chapter-info">
        <div class="chapter-time">${time}</div>
        <div class="chapter-meta">
          <span class="chapter-entries">📝 ${entryCount} ${entryCount === 1 ? "entry" : "entries"}</span>
        </div>
      </div>
    `;

    // Click handler: scroll to first entry in this chapter
    item.addEventListener("click", () => {
      scrollToChapter(chapter);
    });

    if (chaptersList) {
      chaptersList.appendChild(item);
    }
  });
}

/**
 * Format timestamp in HH:MM:SS or MM:SS format
 */
function formatTimestamp(timestampMs: number): string {
  const date = new Date(timestampMs);
  const hours = date.getHours().toString().padStart(2, "0");
  const minutes = date.getMinutes().toString().padStart(2, "0");
  const seconds = date.getSeconds().toString().padStart(2, "0");

  // Only show hours if non-zero
  if (parseInt(hours) > 0) {
    return `${hours}:${minutes}:${seconds}`;
  }
  return `${minutes}:${seconds}`;
}


/**
 * Scroll to the first entry in a chapter
 */
function scrollToChapter(chapter: Chapter): void {
  if (!historyList) return;

  // Find entries matching this chapter's timestamp
  // For now, just scroll to the first entry
  // TODO: Store timestamps on elements to find closest match
  const allEntries = historyList.querySelectorAll<HTMLElement>('[data-entry-id]');
  let closestEntry: HTMLElement | null = null;

  allEntries.forEach((entry) => {
    if (!closestEntry) {
      closestEntry = entry;
    }
  });

  if (closestEntry) {
    const entry: HTMLElement = closestEntry;
    // Scroll into view with smooth behavior
    entry.scrollIntoView({
      behavior: "smooth",
      block: "start",
    });

    // Highlight briefly
    entry.style.transition = "background 0.3s ease";
    entry.style.background = "rgba(29, 166, 160, 0.2)";
    setTimeout(() => {
      entry.style.background = "";
    }, 1000);
  }

  // Update active chapter
  setActiveChapter(chapter.id);
}

/**
 * Set the active chapter (visually highlight in chapter list)
 */
function setActiveChapter(chapterId: string): void {
  if (!chaptersList) return;

  activeChapterId = chapterId;

  // Remove active class from all items
  const items = chaptersList.querySelectorAll(".chapter-item");
  items.forEach((item) => {
    item.classList.remove("active");
  });

  // Add active class to the clicked item
  const activeItem = chaptersList.querySelector(
    `[data-chapter-id="${chapterId}"]`
  );
  if (activeItem) {
    activeItem.classList.add("active");
  }
}

/**
 * Toggle chapters visibility
 */
function toggleChaptersVisibility(): void {
  if (!chaptersContainer || !chaptersToggle) return;

  isChaptersVisible = !isChaptersVisible;

  if (isChaptersVisible) {
    chaptersContainer.style.display = "block";
    chaptersToggle.textContent = "Hide";
    chaptersToggle.title = "Hide chapters";
  } else {
    chaptersContainer.style.display = "none";
    chaptersToggle.textContent = "Show";
    chaptersToggle.title = "Show chapters";
  }
}

/**
 * Refresh chapters (call when history changes)
 */
export function refreshChapters(): void {
  regenerateChapters();
}

/**
 * Get current chapters
 */
export function getCurrentChapters(): Chapter[] {
  return currentChapters;
}
