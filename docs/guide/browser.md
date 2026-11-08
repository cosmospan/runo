---
title: Browser tab
keywords: browser, dev server, localhost, preview, chrome, chromium, web page, job browser, browser tools, playwright, test the app
shortcuts: open-browser
open: settings:general
---
Turn on **Built-in browser** in **Settings → General**. The Browser tab then shows a live page from a Chromium-based browser on your Mac, beside the conversation. ⇧⌘U opens it.

Before a page is open it lists the dev servers running on your machine (or on the session's connection, forwarded to yours).

## A job's own browser

Every job also has a browser of its own, inside Squad. Its bot opens and tests web pages there with Squad's browser tools (open, look, click, type, press, back, screenshot and run JavaScript) instead of opening an outside browser or installing Playwright. Every engine can use it: those that take MCP servers (Claude Code, Codex, Gemini, Cursor, Copilot, OpenCode, Qwen, Amp, Droid) get the tools, and the rest, like Grok, run the same steps as commands (`squad browser open|look|click|type|press|back|screenshot|eval`). A page is read as a short text outline with a number on each link, button and field, and after each step the bot hears only what changed, along with errors the page logged and requests that failed, so trying a page costs few tokens. It asks for a screenshot only when how the page looks matters.

Click **Browser** on a job to watch that page live. You can click, scroll and type into it too, so you can step in while the bot works or try the app yourself. Links to localhost in a job's messages open there, and **Open outside** opens the page in your usual browser.

The browser is the Chromium-based browser on your computer (Chrome, Edge, Brave, Chromium and the like), run headless. For a job on your server it runs on the server and is shown here through Squad's SSH connection, so the server needs no desktop. A job's browser closes when the job is approved or thrown away, or after half an hour that neither the bot nor you used it.

