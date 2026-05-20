# Aurora Website Compatibility Targets

A short list of five websites to use as Aurora demo and compatibility milestones.

## 1. Hacker News

**URL:** https://news.ycombinator.com/

Mandatory target. Hacker News is ideal because it uses old-school HTML, tables, links, comments, forms, and nested pages. It is a strong test for DOM parsing, CSS basics, text layout, navigation, and interaction.

## 2. Example.com

**URL:** https://example.com/

The baseline sanity test. If this page breaks, something fundamental in parsing, layout, networking, or rendering is probably broken.

## 3. Wikipedia Article Page

**Example URL:** https://en.wikipedia.org/wiki/Rust_(programming_language)

A good content-heavy target. It tests long documents, headings, links, images, tables, infoboxes, scrolling, and readable typography.

## 4. MDN Documentation Page

**Example URL:** https://developer.mozilla.org/en-US/docs/Web/API/Document_Object_Model

A useful modern documentation target. It tests semantic HTML, code blocks, sidebars, nested layouts, and responsive page structure.

## 5. GitHub Repository Page

**Example URL:** https://github.com/JohannaWeb/ProjectFalcon

The credibility target. If Aurora can render a GitHub repository page decently, it immediately shows that the engine is handling a more modern, complex real-world page.

## Recommended Milestone Order

```text
Example.com → Hacker News → Wikipedia → MDN → GitHub