# GDD Module Workflow

Last updated: 2026-03-04

This guide describes the current end-to-end workflow in Trisper Flow for generating and publishing Game Design Documents.

## Prerequisites

1. Module `gdd` is enabled in the `Modules` tab.
2. If publishing to Confluence is needed:
   - Configure Confluence auth (`OAuth` or `API token`).
   - Ensure `site_base_url` and (for OAuth) `oauth_cloud_id` are set.
   - Set a default space key if possible.

## Flow

1. Open `Modules` tab and click `Open GDD Flow`.
2. Select a GDD preset (or run `Auto-detect Preset`).
3. Optional: load template guidance from:
   - Confluence page URL
   - File upload (`.pdf`, `.docx`, `.txt`, `.md`)
4. Click `Generate Draft`.
5. Click `Validate`.
6. Optional: click `Suggest Target` for Confluence routing hints.
7. Click `Publish to Confluence`.
8. Use the generated link to open the published page.

## Notes

1. Template ingestion is used as guidance context; it does not replace preset schema enforcement.
2. `.doc` legacy Word format is not supported yet; use `.docx`.
3. Long template text is truncated for safety before prompt usage.

## Troubleshooting

1. `Could not extract Confluence page id from URL`:
   - Ensure the URL includes either `pageId=` or `/pages/<id>/...`.
2. `Confluence OAuth site is not selected yet`:
   - Re-run OAuth exchange and persist selected site.
3. `Could not extract readable text from PDF`:
   - PDF may be image-only. Use OCR or export text first.
