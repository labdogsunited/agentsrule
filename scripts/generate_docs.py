#!/usr/bin/env python3
"""
Emerge Language Documentation Generator
Produces a Houdini VEX-style HTML Help system for the Emerge Language (.em).
Generates docs/index.html (interactive SPA), individual function and guide pages,
CSS stylesheet, and client-side search index.
"""

import os
import json
import html
import re
from doc_data import FUNCTIONS, GUIDE_TOPICS, ATTRIBUTES, DIAGNOSTICS

DOCS_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "docs"))

CSS_CONTENT = """/* SideFX Houdini Help Style for Emerge Language (.em) */
:root {
  --bg-darkest: #131518;
  --bg-dark: #1a1c21;
  --bg-panel: #22252c;
  --bg-panel-hover: #292d36;
  --bg-card: #181a1f;
  --border-subtle: #2d313b;
  --border-strong: #3e4452;
  --text-main: #e2e5eb;
  --text-muted: #9aa1ad;
  --text-dim: #6b7382;
  
  /* Houdini Brand Accents */
  --houdini-orange: #f38230;
  --houdini-orange-hover: #ff984a;
  --houdini-amber: #e0a458;
  --link-blue: #5dade2;
  --link-blue-hover: #85c1e9;
  
  /* Badges & Tags */
  --badge-context-bg: #1a2f42;
  --badge-context-border: #2c4a6b;
  --badge-context-text: #7eb7f5;
  
  --badge-phase-bg: #30203f;
  --badge-phase-border: #4d3463;
  --badge-phase-text: #cda3f2;
  
  --badge-conserved-bg: #3b2216;
  --badge-conserved-border: #633928;
  --badge-conserved-text: #f09a6e;
  
  --badge-gpu-bg: #193828;
  --badge-gpu-border: #26553d;
  --badge-gpu-text: #7cd9a8;
  
  --badge-type-bg: #272d21;
  --badge-type-border: #3d4a31;
  --badge-type-text: #a8d97e;

  /* Syntax Highlighting */
  --syn-keyword: #c678dd;
  --syn-type: #56b6c2;
  --syn-func: #61afef;
  --syn-attr: #e5c07b;
  --syn-amount: #d19a66;
  --syn-rate: #d19a66;
  --syn-string: #98c379;
  --syn-comment: #707886;
  --syn-label: #e06c75;
}

* { box-sizing: border-box; margin: 0; padding: 0; }

body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  font-size: 14px;
  line-height: 1.55;
  color: var(--text-main);
  background-color: var(--bg-dark);
  overflow-x: hidden;
}

a {
  color: var(--link-blue);
  text-decoration: none;
}
a:hover {
  color: var(--link-blue-hover);
  text-decoration: underline;
}

/* Header */
.houdini-header {
  height: 52px;
  background-color: var(--bg-darkest);
  border-bottom: 2px solid var(--houdini-orange);
  display: flex;
  align-items: center;
  padding: 0 16px;
  position: sticky;
  top: 0;
  z-index: 100;
  gap: 16px;
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  text-decoration: none;
  font-weight: 700;
  letter-spacing: 0.04em;
  color: #fff;
  white-space: nowrap;
}
.brand .logo-box {
  width: 28px;
  height: 28px;
  background: linear-gradient(135deg, var(--houdini-orange), #d46316);
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 900;
  font-size: 16px;
  color: #fff;
  box-shadow: 0 2px 6px rgba(243, 130, 48, 0.35);
}
.brand .title { font-size: 16px; color: #fff; }
.brand .version {
  font-size: 11px;
  background: var(--bg-panel);
  padding: 2px 6px;
  border-radius: 10px;
  color: var(--houdini-amber);
  border: 1px solid var(--border-subtle);
}

.search-container {
  flex: 1;
  max-width: 600px;
  position: relative;
  margin: 0 auto;
}
.search-input {
  width: 100%;
  background-color: var(--bg-panel);
  border: 1px solid var(--border-subtle);
  border-radius: 5px;
  padding: 7px 12px 7px 34px;
  color: var(--text-main);
  font-size: 13px;
  outline: none;
  transition: all 0.15s ease;
}
.search-input:focus {
  border-color: var(--houdini-orange);
  background-color: var(--bg-dark);
  box-shadow: 0 0 0 2px rgba(243, 130, 48, 0.2);
}
.search-icon {
  position: absolute;
  left: 10px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--text-dim);
  font-size: 13px;
  pointer-events: none;
}
.search-shortcut {
  position: absolute;
  right: 10px;
  top: 50%;
  transform: translateY(-50%);
  background: var(--bg-darkest);
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  font-size: 10px;
  padding: 1px 5px;
  color: var(--text-dim);
  pointer-events: none;
}

.search-results-dropdown {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  background-color: var(--bg-panel);
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  max-height: 480px;
  overflow-y: auto;
  box-shadow: 0 12px 30px rgba(0,0,0,0.5);
  display: none;
  z-index: 1000;
}
.search-results-dropdown.active { display: block; }
.search-result-item {
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-subtle);
  cursor: pointer;
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.search-result-item:hover, .search-result-item.selected {
  background-color: var(--bg-panel-hover);
}
.search-result-item .item-title {
  font-weight: 600;
  color: var(--text-main);
}
.search-result-item .item-desc {
  font-size: 12px;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 320px;
}
.search-result-item .item-cat {
  font-size: 10px;
  padding: 2px 6px;
  border-radius: 3px;
  background: var(--bg-card);
  color: var(--houdini-amber);
}

.header-links {
  display: flex;
  gap: 16px;
  align-items: center;
}
.header-link {
  font-size: 13px;
  color: var(--text-muted);
  font-weight: 500;
}
.header-link:hover {
  color: #fff;
  text-decoration: none;
}

/* Layout */
.houdini-layout {
  display: flex;
  min-height: calc(100vh - 52px);
}

/* Sidebar */
.houdini-sidebar {
  width: 290px;
  background-color: var(--bg-darkest);
  border-right: 1px solid var(--border-subtle);
  overflow-y: auto;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
}

.sidebar-header {
  padding: 12px 16px 8px;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--text-dim);
  border-bottom: 1px solid var(--border-subtle);
}

.nav-section {
  border-bottom: 1px solid var(--border-subtle);
}
.nav-section-title {
  padding: 10px 16px;
  font-size: 12px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-muted);
  display: flex;
  justify-content: space-between;
  align-items: center;
  cursor: pointer;
  user-select: none;
}
.nav-section-title:hover {
  color: #fff;
  background-color: rgba(255,255,255,0.02);
}
.nav-section-title .count {
  font-size: 10px;
  background: var(--bg-panel);
  padding: 1px 6px;
  border-radius: 8px;
  color: var(--text-dim);
}

.nav-list {
  list-style: none;
  padding: 0 0 6px 0;
}
.nav-item {
  margin: 1px 0;
}
.nav-link {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 5px 16px 5px 22px;
  font-size: 13px;
  color: var(--text-muted);
  text-decoration: none;
  border-left: 3px solid transparent;
}
.nav-link:hover {
  color: #fff;
  background-color: var(--bg-panel);
  text-decoration: none;
}
.nav-link.active {
  color: #fff;
  background-color: var(--bg-panel);
  border-left-color: var(--houdini-orange);
  font-weight: 600;
}
.nav-link .tag {
  font-size: 10px;
  color: var(--text-dim);
  font-family: ui-monospace, monospace;
}

/* Main Content Area */
.houdini-main {
  flex: 1;
  padding: 24px 36px 60px;
  max-width: 1080px;
  margin: 0 auto;
  min-width: 0;
}

/* Breadcrumbs */
.breadcrumbs {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-dim);
  margin-bottom: 16px;
}
.breadcrumbs a { color: var(--text-muted); }
.breadcrumbs a:hover { color: #fff; }
.breadcrumbs .sep { color: var(--text-dim); }

/* Document Header */
.doc-header {
  margin-bottom: 24px;
  border-bottom: 1px solid var(--border-subtle);
  padding-bottom: 18px;
}
.doc-title {
  font-size: 28px;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: #fff;
  margin-bottom: 8px;
  display: flex;
  align-items: center;
  gap: 12px;
}
.doc-title code {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 26px;
  color: var(--houdini-orange);
}
.doc-summary {
  font-size: 16px;
  line-height: 1.5;
  color: var(--text-muted);
  margin-bottom: 14px;
}

/* Badges Bar */
.badge-bar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 9px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.02em;
  text-transform: uppercase;
}
.badge-context {
  background-color: var(--badge-context-bg);
  border: 1px solid var(--badge-context-border);
  color: var(--badge-context-text);
}
.badge-phase {
  background-color: var(--badge-phase-bg);
  border: 1px solid var(--badge-phase-border);
  color: var(--badge-phase-text);
}
.badge-conserved {
  background-color: var(--badge-conserved-bg);
  border: 1px solid var(--badge-conserved-border);
  color: var(--badge-conserved-text);
}
.badge-gpu {
  background-color: var(--badge-gpu-bg);
  border: 1px solid var(--badge-gpu-border);
  color: var(--badge-gpu-text);
}
.badge-type {
  background-color: var(--badge-type-bg);
  border: 1px solid var(--badge-type-border);
  color: var(--badge-type-text);
}

/* Section Headings */
h2 {
  font-size: 18px;
  font-weight: 700;
  color: #fff;
  margin: 32px 0 12px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--border-subtle);
  letter-spacing: 0.01em;
  display: flex;
  align-items: center;
  gap: 8px;
}
h2::before {
  content: "";
  display: inline-block;
  width: 4px;
  height: 16px;
  background-color: var(--houdini-orange);
  border-radius: 2px;
}
h3 {
  font-size: 15px;
  font-weight: 600;
  color: #fff;
  margin: 20px 0 8px;
}
p {
  margin-bottom: 12px;
  color: var(--text-main);
}
ul, ol {
  margin: 8px 0 16px 20px;
}
li {
  margin-bottom: 6px;
}

/* Code and Synopsis */
.synopsis-box {
  background-color: var(--bg-card);
  border: 1px solid var(--border-subtle);
  border-radius: 6px;
  overflow: hidden;
  margin: 16px 0 24px;
  box-shadow: 0 4px 12px rgba(0,0,0,0.25);
}
.synopsis-header {
  background-color: #121417;
  padding: 6px 12px;
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  color: var(--text-dim);
  border-bottom: 1px solid var(--border-subtle);
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.synopsis-content {
  padding: 14px 16px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 13px;
  line-height: 1.6;
  overflow-x: auto;
  color: #dcdfe4;
}

pre {
  background-color: var(--bg-card);
  border: 1px solid var(--border-subtle);
  border-radius: 6px;
  padding: 14px 16px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 13px;
  line-height: 1.55;
  overflow-x: auto;
  margin: 12px 0 20px;
  color: #abb2bf;
}
code {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 0.9em;
  background-color: rgba(255,255,255,0.07);
  padding: 2px 5px;
  border-radius: 3px;
  color: #e5c07b;
}
pre code {
  background: transparent;
  padding: 0;
  border-radius: 0;
  color: inherit;
}

/* Syntax Highlighting Tokens */
.k { color: var(--syn-keyword); font-weight: 600; }
.t { color: var(--syn-type); }
.f { color: var(--syn-func); font-weight: 600; }
.a { color: var(--syn-attr); font-weight: 600; }
.n { color: var(--syn-amount); }
.s { color: var(--syn-string); }
.c { color: var(--syn-comment); font-style: italic; }
.l { color: var(--syn-label); }

/* Tables */
.houdini-table {
  width: 100%;
  border-collapse: collapse;
  margin: 16px 0 24px;
  font-size: 13px;
}
.houdini-table th {
  background-color: var(--bg-card);
  color: var(--text-muted);
  font-weight: 700;
  text-transform: uppercase;
  font-size: 11px;
  letter-spacing: 0.05em;
  padding: 8px 12px;
  text-align: left;
  border: 1px solid var(--border-subtle);
}
.houdini-table td {
  padding: 8px 12px;
  border: 1px solid var(--border-subtle);
  background-color: var(--bg-dark);
  vertical-align: top;
  line-height: 1.45;
}
.houdini-table tr:nth-child(even) td {
  background-color: rgba(255,255,255,0.015);
}
.houdini-table tr:hover td {
  background-color: var(--bg-panel-hover);
}
.houdini-table .arg-name {
  font-family: ui-monospace, monospace;
  font-weight: 600;
  color: #e5c07b;
  white-space: nowrap;
}
.houdini-table .arg-type {
  font-family: ui-monospace, monospace;
  color: var(--syn-type);
  font-size: 12px;
  white-space: nowrap;
}

/* Table Lists and Cell Bullets */
.cell-list, .houdini-table td ul {
  margin: 2px 0 2px 16px;
  padding: 0;
  list-style-type: disc;
}
.cell-list li, .houdini-table td li {
  margin: 3px 0;
  line-height: 1.45;
  color: var(--text-main);
}
.cell-list li::marker, .houdini-table td li::marker {
  color: var(--houdini-orange);
}
.cell-list li:first-child, .houdini-table td li:first-child {
  margin-top: 1px;
}
.cell-list li:last-child, .houdini-table td li:last-child {
  margin-bottom: 1px;
}
.cell-list code, .houdini-table td code {
  font-size: 0.9em;
  padding: 1px 4px;
}
.houdini-table td br {
  margin-bottom: 4px;
}

/* Guide Body Lists */
.guide-list {
  margin: 8px 0 16px 24px;
  padding: 0;
  line-height: 1.6;
  color: var(--text-main);
}
.guide-list li {
  margin: 4px 0;
}
.guide-list li::marker {
  color: var(--houdini-orange);
}

/* Diagrams and Visual Figures */
.diagram-figure {
  margin: 28px 0;
  background-color: var(--bg-darkest);
  border: 1px solid var(--border-subtle);
  border-radius: 8px;
  overflow: hidden;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.45);
}
.diagram-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 9px 16px;
  background-color: var(--bg-panel);
  border-bottom: 1px solid var(--border-subtle);
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-muted);
}
.diagram-header .diagram-tag {
  background-color: rgba(243, 130, 48, 0.15);
  color: var(--houdini-orange);
  padding: 2px 7px;
  border-radius: 3px;
  font-size: 10px;
  font-weight: 700;
  border: 1px solid rgba(243, 130, 48, 0.3);
}
.diagram-svg-wrap {
  padding: 20px 16px;
  background: radial-gradient(#252a35 1px, transparent 1px);
  background-size: 20px 20px;
  background-color: #13161c;
  overflow-x: auto;
  text-align: center;
}
.diagram-figure svg {
  display: inline-block;
  max-width: 100%;
  height: auto;
  vertical-align: middle;
}
.diagram-caption {
  padding: 12px 18px;
  background-color: var(--bg-panel);
  border-top: 1px solid var(--border-subtle);
  font-size: 12px;
  color: var(--text-muted);
  line-height: 1.55;
}
.diagram-caption strong {
  color: var(--text-main);
}


/* Admonitions / Callouts (Houdini Style) */
.admonition {
  margin: 18px 0;
  border-radius: 6px;
  border-left: 4px solid;
  padding: 12px 16px;
  background-color: var(--bg-card);
}
.admonition-title {
  font-weight: 700;
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  margin-bottom: 6px;
  display: flex;
  align-items: center;
  gap: 8px;
}
.admonition.note {
  border-left-color: #3b82f6;
  background-color: #172233;
}
.admonition.note .admonition-title { color: #60a5fa; }

.admonition.tip {
  border-left-color: #10b981;
  background-color: #122820;
}
.admonition.tip .admonition-title { color: #34d399; }

.admonition.warning {
  border-left-color: #f59e0b;
  background-color: #2b2413;
}
.admonition.warning .admonition-title { color: #fbbf24; }

.admonition.law {
  border-left-color: #ef4444;
  background-color: #2b1618;
}
.admonition.law .admonition-title { color: #f87171; }

/* See Also Links */
.see-also-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 10px;
  margin: 16px 0;
}
.see-also-card {
  background-color: var(--bg-panel);
  border: 1px solid var(--border-subtle);
  border-radius: 5px;
  padding: 10px 12px;
  display: block;
  text-decoration: none;
  transition: all 0.15s ease;
}
.see-also-card:hover {
  background-color: var(--bg-panel-hover);
  border-color: var(--houdini-orange);
  text-decoration: none;
}
.see-also-card .card-title {
  font-weight: 600;
  color: var(--text-main);
  font-family: ui-monospace, monospace;
}
.see-also-card .card-desc {
  font-size: 12px;
  color: var(--text-muted);
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* On This Page (TOC) */
.houdini-toc {
  width: 200px;
  flex-shrink: 0;
  padding: 24px 16px;
  position: sticky;
  top: 52px;
  height: calc(100vh - 52px);
  overflow-y: auto;
  border-left: 1px solid var(--border-subtle);
  font-size: 12px;
}
.toc-title {
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--text-dim);
  margin-bottom: 10px;
}
.toc-list {
  list-style: none;
}
.toc-item {
  margin: 6px 0;
}
.toc-link {
  color: var(--text-muted);
  display: block;
  padding: 2px 0;
}
.toc-link:hover, .toc-link.active {
  color: var(--houdini-orange);
  text-decoration: none;
}

/* Copy button */
.copy-btn {
  background: var(--bg-panel);
  border: 1px solid var(--border-strong);
  border-radius: 4px;
  color: var(--text-muted);
  font-size: 10px;
  padding: 2px 8px;
  cursor: pointer;
  transition: all 0.15s;
}
.copy-btn:hover {
  color: #fff;
  border-color: var(--houdini-orange);
}

@media (max-width: 960px) {
  .houdini-toc { display: none; }
  .houdini-main { padding: 18px 20px 40px; }
}
@media (max-width: 720px) {
  .houdini-sidebar { display: none; }
}
"""

JS_CONTENT = """// Emerge Language Documentation Interactive Client
document.addEventListener('DOMContentLoaded', () => {
  const searchInput = document.getElementById('search-input');
  const searchDropdown = document.getElementById('search-dropdown');
  
  // Keyboard Shortcut: '/' to search, 'Escape' to dismiss
  window.addEventListener('keydown', (e) => {
    if (e.key === '/' && document.activeElement !== searchInput) {
      e.preventDefault();
      searchInput.focus();
      searchInput.select();
    } else if (e.key === 'Escape') {
      if (searchDropdown) searchDropdown.classList.remove('active');
      if (searchInput) searchInput.blur();
    }
  });

  // Search logic
  if (searchInput && searchDropdown && window.EMERGE_SEARCH_INDEX) {
    searchInput.addEventListener('input', (e) => {
      const q = e.target.value.trim().toLowerCase();
      if (!q) {
        searchDropdown.classList.remove('active');
        searchDropdown.innerHTML = '';
        return;
      }

      const matches = window.EMERGE_SEARCH_INDEX.filter(item => {
        return item.title.toLowerCase().includes(q) ||
               item.desc.toLowerCase().includes(q) ||
               (item.keywords && item.keywords.toLowerCase().includes(q));
      }).slice(0, 12);

      if (matches.length === 0) {
        searchDropdown.innerHTML = '<div style="padding:10px 14px;color:var(--text-dim);font-size:12px;">No matching functions or topics found</div>';
      } else {
        searchDropdown.innerHTML = matches.map(item => `
          <div class="search-result-item" onclick="window.location.href='${item.url}'">
            <div>
              <div class="item-title">${item.title}</div>
              <div class="item-desc">${item.desc}</div>
            </div>
            <span class="item-cat">${item.cat}</span>
          </div>
        `).join('');
      }
      searchDropdown.classList.add('active');
    });

    document.addEventListener('click', (e) => {
      if (!searchInput.contains(e.target) && !searchDropdown.contains(e.target)) {
        searchDropdown.classList.remove('active');
      }
    });
  }

  // Copy code snippets
  document.querySelectorAll('.copy-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const targetId = btn.getAttribute('data-target');
      const el = document.getElementById(targetId);
      if (el) {
        navigator.clipboard.writeText(el.innerText).then(() => {
          const original = btn.innerText;
          btn.innerText = 'COPIED!';
          setTimeout(() => { btn.innerText = original; }, 1500);
        });
      }
    });
  });

  // Collapsible sidebar sections
  document.querySelectorAll('.nav-section-title').forEach(header => {
    header.addEventListener('click', () => {
      const list = header.nextElementSibling;
      if (list) {
        const isCollapsed = list.style.display === 'none';
        list.style.display = isCollapsed ? 'block' : 'none';
      }
    });
  });

  // Hash router support for Single-Page Application browsing on index.html
  if (window.EMERGE_DOCS_STORE) {
    function loadPageFromHash() {
      const hash = window.location.hash.replace('#', '') || 'guide/overview';
      const pageData = window.EMERGE_DOCS_STORE[hash];
      if (pageData) {
        const mainEl = document.getElementById('main-content-pane');
        if (mainEl) {
          mainEl.innerHTML = pageData.html;
          document.title = pageData.title + ' — Emerge Documentation';
          // Update active links in sidebar
          document.querySelectorAll('.nav-link').forEach(link => {
            if (link.getAttribute('href') === '#' + hash || link.getAttribute('data-id') === hash) {
              link.classList.add('active');
            } else {
              link.classList.remove('active');
            }
          });
          window.scrollTo(0, 0);
        }
      }
    }
    window.addEventListener('hashchange', loadPageFromHash);
    loadPageFromHash();
  }
});
"""

def ensure_dirs():
    os.makedirs(DOCS_DIR, exist_ok=True)
    os.makedirs(os.path.join(DOCS_DIR, "functions"), exist_ok=True)
    os.makedirs(os.path.join(DOCS_DIR, "guide"), exist_ok=True)

def highlight_emerge_code(code_str):
    lines = code_str.strip().split("\n")
    out_lines = []
    keywords = r"\b(world|rule|on|when|else|let|for|in|substance|reaction|field|context|if|signal|source|sink|waste|remains|decompose|upkeep|limits|senses|actions|regulation|gene|module|seed|genome|lineage|extends|const|dormant|requires|cost|per|keep|threshold|view|layer|chart|explore|patch|self|true|false|and|or|not|where|states|combine|holds|catalyze)\b"
    types = r"\b(amount|rate|int|f32|bool|vec2|float|vector2|color|string|list|store|neighbour|collection|Continuous2D|Grid)\b"

    for line in lines:
        comment_part = ""
        code_part = line
        if "//" in line:
            idx = line.find("//")
            code_part = line[:idx]
            comment_part = line[idx:]

        code_part = html.escape(code_part)
        if comment_part:
            comment_part = f'<span class="c">{html.escape(comment_part)}</span>'

        code_part = re.sub(r'(&quot;.*?&quot;|".*?")', r'<span class="s">\1</span>', code_part)
        code_part = re.sub(r'(@[A-Za-z_][A-Za-z0-9_]*)', r'<span class="a">\1</span>', code_part)
        code_part = re.sub(keywords, r'<span class="k">\1</span>', code_part)
        code_part = re.sub(types, r'<span class="t">\1</span>', code_part)
        code_part = re.sub(r'\b(\d+(?:_\d+)*(?:\.\d+)?(?:mu|u|bp)?)\b', r'<span class="n">\1</span>', code_part)

        out_lines.append(code_part + comment_part)

    return "\n".join(out_lines)

def escape_code(text):
    return highlight_emerge_code(text)

def render_sidebar(current_id="", base_url=""):
    # Group functions by category
    cats = {}
    for f in FUNCTIONS:
        cat = f["category"]
        if cat not in cats:
            cats[cat] = []
        cats[cat].append(f)

    html_out = ['<aside class="houdini-sidebar">']
    
    # 1. Guides Section
    html_out.append('<div class="sidebar-header">DOCUMENTATION</div>')
    html_out.append('<div class="nav-section">')
    html_out.append('  <div class="nav-section-title"><span>📖 Language Guide</span><span class="count">%d</span></div>' % len(GUIDE_TOPICS))
    html_out.append('  <ul class="nav-list">')
    for g in GUIDE_TOPICS:
        active_cls = " active" if current_id == ("guide/" + g["id"]) else ""
        href = f"{base_url}guide/{g['id']}.html" if base_url else f"#guide/{g['id']}"
        html_out.append(f'    <li class="nav-item"><a class="nav-link{active_cls}" data-id="guide/{g["id"]}" href="{href}">{g["title"]}</a></li>')
    html_out.append('  </ul>')
    html_out.append('</div>')

    # 2. Reference Sections
    html_out.append('<div class="nav-section">')
    attr_active = " active" if current_id == "attributes" else ""
    attr_href = f"{base_url}attributes.html" if base_url else "#attributes"
    html_out.append(f'  <div class="nav-section-title"><a href="{attr_href}" class="nav-link{attr_active}" style="padding:0;border:0;color:inherit;font:inherit;">🏷️ Attributes Reference</a><span class="count">%d</span></div>' % len(ATTRIBUTES))
    html_out.append('</div>')

    html_out.append('<div class="nav-section">')
    diag_active = " active" if current_id == "diagnostics" else ""
    diag_href = f"{base_url}diagnostics.html" if base_url else "#diagnostics"
    html_out.append(f'  <div class="nav-section-title"><a href="{diag_href}" class="nav-link{diag_active}" style="padding:0;border:0;color:inherit;font:inherit;">⚠️ Diagnostics & Codes</a><span class="count">%d</span></div>' % len(DIAGNOSTICS))
    html_out.append('</div>')

    html_out.append('<div class="nav-section">')
    vex_active = " active" if current_id == "vex-comparison" else ""
    vex_href = f"{base_url}vex-comparison.html" if base_url else "#guide/vex-comparison"
    html_out.append(f'  <div class="nav-section-title"><a href="{vex_href}" class="nav-link{vex_active}" style="padding:0;border:0;color:inherit;font:inherit;">🔄 Houdini VEX Rosetta</a></div>')
    html_out.append('</div>')

    # 3. Categorized Functions Section
    html_out.append('<div class="sidebar-header" style="margin-top:8px;">FUNCTIONS BY CATEGORY</div>')
    for cat_name, funcs in cats.items():
        html_out.append('<div class="nav-section">')
        html_out.append(f'  <div class="nav-section-title"><span>{cat_name}</span><span class="count">{len(funcs)}</span></div>')
        html_out.append('  <ul class="nav-list">')
        for fn in funcs:
            active_cls = " active" if current_id == ("functions/" + fn["id"]) else ""
            href = f"{base_url}functions/{fn['id']}.html" if base_url else f"#functions/{fn['id']}"
            html_out.append(f'    <li class="nav-item"><a class="nav-link{active_cls}" data-id="functions/{fn["id"]}" href="{href}"><span>{fn["name"]}</span><span class="tag">{fn["phase"]}</span></a></li>')
        html_out.append('  </ul>')
        html_out.append('</div>')

    html_out.append('</aside>')
    return "\n".join(html_out)

def render_header(base_url=""):
    home_href = f"{base_url}index.html" if base_url else "index.html"
    return f"""
<header class="houdini-header">
  <a href="{home_href}" class="brand">
    <div class="logo-box">E</div>
    <span class="title">EMERGE</span>
    <span class="version">v0.7.9</span>
  </a>

  <div class="search-container">
    <span class="search-icon">🔍</span>
    <input type="text" id="search-input" class="search-input" placeholder="Search functions, attributes, types, contexts... (Press '/' to focus)" autocomplete="off">
    <span class="search-shortcut">/</span>
    <div id="search-dropdown" class="search-results-dropdown"></div>
  </div>

  <nav class="header-links">
    <a href="{base_url}index.html#guide/overview" class="header-link">Guide</a>
    <a href="{base_url}index.html#functions/transfer" class="header-link">Functions</a>
    <a href="{base_url}index.html#attributes" class="header-link">Attributes</a>
    <a href="{base_url}index.html#guide/vex-comparison" class="header-link">Houdini VEX</a>
  </nav>
</header>
"""

def render_function_content(f, base_url=""):
    # Badges
    badges = []
    badges.append(f'<span class="badge badge-type">{html.escape(f["return_type"])}</span>')
    badges.append(f'<span class="badge badge-context">Context: {html.escape(f["context"])}</span>')
    badges.append(f'<span class="badge badge-phase">Phase: {html.escape(f["phase"])}</span>')
    if f.get("conserved"):
        badges.append('<span class="badge badge-conserved">⚖️ Conserved</span>')
    if f.get("gpu"):
        badges.append('<span class="badge badge-gpu">⚡ GPU Contract</span>')
    badge_bar = " ".join(badges)

    # Synopsis box
    synopsis_html = f"""
<div class="synopsis-box">
  <div class="synopsis-header">
    <span>Synopsis — Emerge Language (.em)</span>
    <button class="copy-btn" data-target="synopsis-code">COPY</button>
  </div>
  <pre class="synopsis-content" id="synopsis-code"><code>{escape_code(f["synopsis"])}</code></pre>
</div>
"""

    # Arguments table
    args_rows = []
    for p in f["params"]:
        args_rows.append(f"""
<tr>
  <td class="arg-name">{html.escape(p["name"])}</td>
  <td class="arg-type">{html.escape(p["type"])}</td>
  <td>{html.escape(p["req"])}</td>
  <td>{html.escape(p["desc"])}</td>
</tr>
""")
    args_table = f"""
<table class="houdini-table">
  <thead>
    <tr>
      <th style="width:18%;">Argument</th>
      <th style="width:20%;">Type</th>
      <th style="width:12%;">Required</th>
      <th>Description</th>
    </tr>
  </thead>
  <tbody>
    {"".join(args_rows)}
  </tbody>
</table>
"""

    # Details Markdown to HTML (simple converter)
    details_html = f["details"].replace("\n\n", "</p><p>").replace("\n", " ")
    # highlight subheadings
    details_html = re.sub(r'### (.*?)(?:</p>|<br>|\n)', r'<h3>\1</h3><p>', details_html)

    # Example box
    example_html = f"""
<div class="synopsis-box">
  <div class="synopsis-header">
    <span>Example — {f["name"]}</span>
    <button class="copy-btn" data-target="example-code">COPY</button>
  </div>
  <pre class="synopsis-content" id="example-code"><code>{escape_code(f["example"])}</code></pre>
</div>
"""

    # See Also cards
    see_also_cards = []
    for s_id in f.get("see_also", []):
        # Find function info
        target = next((x for x in FUNCTIONS if x["id"] == s_id), None)
        if target:
            href = f"{base_url}functions/{target['id']}.html" if base_url else f"#functions/{target['id']}"
            see_also_cards.append(f"""
<a href="{href}" class="see-also-card">
  <div class="card-title">{target["name"]}</div>
  <div class="card-desc">{target["summary"]}</div>
</a>
""")
        else:
            see_also_cards.append(f"""
<a href="#{s_id}" class="see-also-card">
  <div class="card-title">{s_id}</div>
  <div class="card-desc">Related topic</div>
</a>
""")

    content = f"""
<nav class="breadcrumbs">
  <a href="{base_url}index.html">Help</a>
  <span class="sep">></span>
  <a href="{base_url}index.html#functions/transfer">Emerge Functions</a>
  <span class="sep">></span>
  <span>{f["category"]}</span>
  <span class="sep">></span>
  <span style="color:#fff;">{f["name"]}</span>
</nav>

<header class="doc-header">
  <h1 class="doc-title"><code>{f["name"]}</code></h1>
  <p class="doc-summary">{html.escape(f["summary"])}</p>
  <div class="badge-bar">
    {badge_bar}
  </div>
</header>

<section id="synopsis">
  <h2>Synopsis</h2>
  {synopsis_html}
</section>

<section id="arguments">
  <h2>Arguments</h2>
  {args_table}
</section>

<section id="returns">
  <h2>Returns</h2>
  <p><code>{html.escape(f["return_type"])}</code> — {html.escape(f["returns"])}</p>
</section>

<section id="details">
  <h2>Details</h2>
  <div class="details-body">
    <p>{details_html}</p>
  </div>
</section>

<section id="examples">
  <h2>Examples</h2>
  {example_html}
</section>

<section id="see-also">
  <h2>See Also</h2>
  <div class="see-also-grid">
    {"".join(see_also_cards)}
  </div>
</section>
"""
    return content

def format_table_cell(td):
    # Inline code replacement with HTML escaping inside code
    td = re.sub(r'\*\*(.*?)\*\*', r'<strong>\1</strong>', td)
    td = re.sub(r'`(.*?)`', lambda m: f'<code>{html.escape(m.group(1))}</code>', td)
    
    # Check if td has bullet points indicated by • or <br>
    if '<br>' in td or '•' in td:
        parts = [p.strip() for p in re.split(r'<br\s*/?>', td) if p.strip()]
        if any(p.startswith('•') or p.startswith('-') for p in parts) or len(parts) > 1:
            items = []
            for p in parts:
                clean_p = re.sub(r'^[•\-]\s*', '', p)
                items.append(f'<li>{clean_p}</li>')
            return f'<ul class="cell-list">{"".join(items)}</ul>'
        else:
            return '<br>'.join(parts)
            
    # Check if td has semicolon-separated items (e.g. "a; b; c")
    # If it has 1 or more semicolons, split into a clean bulleted list!
    if ';' in td and not any(entity in td for entity in ['&quot;', '&#39;', '&amp;', '&lt;', '&gt;']):
        parts = [p.strip() for p in td.split(';') if p.strip()]
        if len(parts) > 1:
            items = [f'<li>{p}</li>' for p in parts]
            return f'<ul class="cell-list">{"".join(items)}</ul>'
        
    return td

def render_guide_content(g, base_url=""):
    # Convert markdown headers and codeblocks to clean HTML
    raw_md = g["content"]
    
    # 1. Protect raw HTML figures, SVGs, and admonitions from paragraph splitting
    raw_blocks = []
    def save_raw_block(m):
        idx = len(raw_blocks)
        raw_blocks.append(m.group(0))
        return f'\n\n<!--RAW_BLOCK_{idx}-->\n\n'

    body = re.sub(r'<figure\b.*?</figure>', save_raw_block, raw_md, flags=re.DOTALL)
    body = re.sub(r'<div class="admonition\b.*?</div>', save_raw_block, body, flags=re.DOTALL)
    
    # 2. Process code blocks and protect them as blocks so blank lines inside code don't split
    def code_repl(m):
        code_text = escape_code(m.group(1))
        idx = len(raw_blocks)
        code_html = f'<div class="synopsis-box"><pre class="synopsis-content"><code>{code_text}</code></pre></div>'
        raw_blocks.append(code_html)
        return f'\n\n<!--RAW_BLOCK_{idx}-->\n\n'
    
    body = re.sub(r'```(?:emerge|ebnf|c|sh|json)?\n(.*?)```', code_repl, body, flags=re.DOTALL)
    
    # 3. Process headings in order of depth: ###, then ##, then #
    body = re.sub(r'(?m)^### (.*?)$', r'\n\n<h3>\1</h3>\n\n', body)
    body = re.sub(r'(?m)^## (.*?)$', r'\n\n<h2>\1</h2>\n\n', body)
    body = re.sub(r'(?m)^# (.*?)$', r'\n\n<header class="doc-header"><h1 class="doc-title">\1</h1></header>\n\n', body)
    
    # 4. Ensure list blocks are cleanly separated from surrounding text paragraphs
    body = re.sub(r'(?m)(^[^*-0-9\n].*)\n([*-] |\d+\. )', r'\1\n\n\2', body)
    body = re.sub(r'(?m)(^[*-] .*|^\d+\. .*)\n([^*-0-9\n])', r'\1\n\n\2', body)
    
    # 5. Split into paragraphs
    paragraphs = body.split("\n\n")
    formatted_p = []
    for p in paragraphs:
        p = p.strip()
        if not p:
            continue
        if p.startswith("<!--RAW_BLOCK_"):
            m_block = re.search(r'\d+', p)
            if m_block:
                idx = int(m_block.group(0))
                formatted_p.append(raw_blocks[idx])
        elif p.startswith("<h") or p.startswith("<header"):
            formatted_p.append(p)
        elif p.startswith("|"):
            # Markdown table to HTML with cell formatting
            lines = [line.strip() for line in p.split("\n") if line.strip()]
            if len(lines) >= 2:
                headers = [h.strip() for h in lines[0].split("|")[1:-1]]
                table_html = ['<table class="houdini-table"><thead><tr>']
                for th in headers:
                    table_html.append(f'<th>{html.escape(th)}</th>')
                table_html.append('</tr></thead><tbody>')
                for row_line in lines[2:]:
                    cols = [c.strip() for c in row_line.split("|")[1:-1]]
                    table_html.append('<tr>')
                    for td in cols:
                        formatted_td = format_table_cell(td)
                        table_html.append(f'<td>{formatted_td}</td>')
                    table_html.append('</tr>')
                table_html.append('</tbody></table>')
                formatted_p.append("".join(table_html))
        elif p.startswith("- ") or p.startswith("* "):
            # Bullet list
            items = []
            for line in p.split("\n"):
                line = line.strip()
                if line.startswith("- ") or line.startswith("* "):
                    item_text = line[2:].strip()
                    item_text = re.sub(r'\*\*(.*?)\*\*', r'<strong>\1</strong>', item_text)
                    item_text = re.sub(r'`(.*?)`', lambda m: f'<code>{html.escape(m.group(1))}</code>', item_text)
                    items.append(f'<li>{item_text}</li>')
                elif line and items:
                    line = re.sub(r'\*\*(.*?)\*\*', r'<strong>\1</strong>', line)
                    line = re.sub(r'`(.*?)`', lambda m: f'<code>{html.escape(m.group(1))}</code>', line)
                    items[-1] = items[-1][:-5] + f' {line}</li>'
            formatted_p.append(f'<ul class="guide-list">{"".join(items)}</ul>')
        elif re.match(r'^\d+\.\s', p):
            # Numbered list
            items = []
            for line in p.split("\n"):
                line = line.strip()
                m_num = re.match(r'^\d+\.\s*(.*)', line)
                if m_num:
                    item_text = m_num.group(1).strip()
                    item_text = re.sub(r'\*\*(.*?)\*\*', r'<strong>\1</strong>', item_text)
                    item_text = re.sub(r'`(.*?)`', lambda m: f'<code>{html.escape(m.group(1))}</code>', item_text)
                    items.append(f'<li>{item_text}</li>')
                elif line and items:
                    line = re.sub(r'\*\*(.*?)\*\*', r'<strong>\1</strong>', line)
                    line = re.sub(r'`(.*?)`', lambda m: f'<code>{html.escape(m.group(1))}</code>', line)
                    items[-1] = items[-1][:-5] + f' {line}</li>'
            formatted_p.append(f'<ol class="guide-list">{"".join(items)}</ol>')
        else:
            # Inline bold / code
            p = re.sub(r'\*\*(.*?)\*\*', r'<strong>\1</strong>', p)
            p = re.sub(r'`(.*?)`', lambda m: f'<code>{html.escape(m.group(1))}</code>', p)
            formatted_p.append(f'<p>{p}</p>')

    body_html = "\n".join(formatted_p)

    content = f"""
<nav class="breadcrumbs">
  <a href="{base_url}index.html">Help</a>
  <span class="sep">></span>
  <a href="{base_url}index.html#guide/overview">Emerge Language Guide</a>
  <span class="sep">></span>
  <span style="color:#fff;">{g["title"]}</span>
</nav>

<div class="guide-content">
  {body_html}
</div>
"""
    return content

def render_attributes_page(base_url=""):
    rows = []
    for a in ATTRIBUTES:
        rows.append(f"""
<tr>
  <td class="arg-name">{html.escape(a["name"])}</td>
  <td class="arg-type">{html.escape(a["type"])}</td>
  <td><span class="badge badge-context">{html.escape(a["domain"])}</span></td>
  <td>{html.escape(a["desc"])}</td>
</tr>
""")

    content = f"""
<nav class="breadcrumbs">
  <a href="{base_url}index.html">Help</a>
  <span class="sep">></span>
  <span>Reference</span>
  <span class="sep">></span>
  <span style="color:#fff;">Attributes Reference</span>
</nav>

<header class="doc-header">
  <h1 class="doc-title">Attributes Reference (<code>@attribute</code>)</h1>
  <p class="doc-summary">Comprehensive reference of agent, cell, world, and visual output attributes in Emerge.</p>
</header>

<div class="admonition tip">
  <div class="admonition-title">💡 VEX PARALLEL EVALUATION</div>
  <p>In VEX-style layer bodies (e.g. <code>context(agents)</code> or <code>context(cells)</code>), attributes prefixed with <code>@</code> access the current element's properties directly on the start-of-tick snapshot. Writing to visual attributes (<code>@Cd</code>, <code>@Alpha</code>, <code>@pscale</code>, <code>@shape</code>, <code>@N</code>) directly controls viewport rendering.</p>
</div>

<table class="houdini-table">
  <thead>
    <tr>
      <th style="width:20%;">Attribute</th>
      <th style="width:18%;">Type</th>
      <th style="width:20%;">Domain</th>
      <th>Description</th>
    </tr>
  </thead>
  <tbody>
    {"".join(rows)}
  </tbody>
</table>
"""
    return content

def render_diagnostics_page(base_url=""):
    rows = []
    for d in DIAGNOSTICS:
        sev_badge = "badge-phase" if d["severity"] == "error" else ("badge-gpu" if d["severity"] == "note" else "badge-conserved")
        rows.append(f"""
<tr>
  <td class="arg-name"><code>{html.escape(d["code"])}</code></td>
  <td><span class="badge {sev_badge}">{html.escape(d["severity"])}</span></td>
  <td><span class="badge badge-context">{html.escape(d["area"])}</span></td>
  <td>{html.escape(d["desc"])}</td>
</tr>
""")

    content = f"""
<nav class="breadcrumbs">
  <a href="{base_url}index.html">Help</a>
  <span class="sep">></span>
  <span>Reference</span>
  <span class="sep">></span>
  <span style="color:#fff;">Compiler Diagnostics & Codes</span>
</nav>

<header class="doc-header">
  <h1 class="doc-title">Compiler Diagnostics & Error Codes</h1>
  <p class="doc-summary">Complete taxonomy of E00xx through E10xx error codes, warnings, and static analysis notes generated by <code>emerge check</code>.</p>
</header>

<div class="admonition note">
  <div class="admonition-title">ℹ️ STRUCTURED JSON DIAGNOSTICS</div>
  <p><code>emerge check --json &lt;file.em&gt;</code> prints machine-readable JSON diagnostics with stable codes, spans, human messages, and automated patch fixes that tools and editors can apply directly.</p>
</div>

<table class="houdini-table">
  <thead>
    <tr>
      <th style="width:15%;">Code</th>
      <th style="width:12%;">Severity</th>
      <th style="width:18%;">Domain Area</th>
      <th>Description & Cause</th>
    </tr>
  </thead>
  <tbody>
    {"".join(rows)}
  </tbody>
</table>
"""
    return content

def build_search_index():
    index = []
    for f in FUNCTIONS:
        index.append({
            "title": f["name"],
            "desc": f["summary"],
            "cat": f["category"],
            "url": f"index.html#functions/{f['id']}",
            "keywords": f"{f['name']} {f['context']} {f['phase']} {f['return_type']}"
        })
    for g in GUIDE_TOPICS:
        index.append({
            "title": g["title"],
            "desc": g["summary"],
            "cat": "Guide",
            "url": f"index.html#guide/{g['id']}",
            "keywords": f"{g['title']} guide grammar syntax types contexts"
        })
    for a in ATTRIBUTES:
        index.append({
            "title": a["name"],
            "desc": a["desc"],
            "cat": "Attribute",
            "url": f"index.html#attributes",
            "keywords": f"{a['name']} {a['domain']} {a['type']} attribute"
        })
    for d in DIAGNOSTICS:
        index.append({
            "title": d["code"],
            "desc": f"{d['area']}: {d['desc']}",
            "cat": "Diagnostic",
            "url": f"index.html#diagnostics",
            "keywords": f"{d['code']} {d['area']} {d['desc']} error warning diagnostic"
        })
    return index

def render_toc_for_function(f):
    return """
<nav class="houdini-toc">
  <div class="toc-title">ON THIS PAGE</div>
  <ul class="toc-list">
    <li class="toc-item"><a class="toc-link" href="#synopsis">Synopsis</a></li>
    <li class="toc-item"><a class="toc-link" href="#arguments">Arguments</a></li>
    <li class="toc-item"><a class="toc-link" href="#returns">Returns</a></li>
    <li class="toc-item"><a class="toc-link" href="#details">Details</a></li>
    <li class="toc-item"><a class="toc-link" href="#examples">Examples</a></li>
    <li class="toc-item"><a class="toc-link" href="#see-also">See Also</a></li>
  </ul>
</nav>
"""

def render_toc_generic():
    return """
<nav class="houdini-toc">
  <div class="toc-title">ON THIS PAGE</div>
  <ul class="toc-list">
    <li class="toc-item"><a class="toc-link" href="#top">Overview</a></li>
  </ul>
</nav>
"""

def generate_static_page(title, content_html, toc_html, current_id, out_path, base_url="../"):
    header = render_header(base_url=base_url)
    sidebar = render_sidebar(current_id=current_id, base_url=base_url)
    
    full_html = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{html.escape(title)} — Emerge Documentation</title>
  <link rel="stylesheet" href="{base_url}style.css">
</head>
<body>
  {header}
  <div class="houdini-layout">
    {sidebar}
    <main class="houdini-main">
      {content_html}
    </main>
    {toc_html}
  </div>
  <script src="{base_url}help.js"></script>
</body>
</html>
"""
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(full_html)

def main():
    ensure_dirs()
    print("Writing style.css and help.js...")
    
    with open(os.path.join(DOCS_DIR, "style.css"), "w", encoding="utf-8") as f:
        f.write(CSS_CONTENT)
    with open(os.path.join(DOCS_DIR, "help.js"), "w", encoding="utf-8") as f:
        f.write(JS_CONTENT)

    search_index = build_search_index()
    docs_store = {}

    print("Generating function pages...")
    for fn in FUNCTIONS:
        f_html = render_function_content(fn, base_url="../")
        toc = render_toc_for_function(fn)
        generate_static_page(
            title=f"{fn['name']} — Emerge Function",
            content_html=f_html,
            toc_html=toc,
            current_id=f"functions/{fn['id']}",
            out_path=os.path.join(DOCS_DIR, "functions", f"{fn['id']}.html"),
            base_url="../"
        )
        # Store for interactive SPA
        docs_store[f"functions/{fn['id']}"] = {
            "title": fn["name"],
            "html": render_function_content(fn, base_url="")
        }

    print("Generating guide pages...")
    for g in GUIDE_TOPICS:
        g_html = render_guide_content(g, base_url="../")
        generate_static_page(
            title=f"{g['title']} — Emerge Guide",
            content_html=g_html,
            toc_html=render_toc_generic(),
            current_id=f"guide/{g['id']}",
            out_path=os.path.join(DOCS_DIR, "guide", f"{g['id']}.html"),
            base_url="../"
        )
        docs_store[f"guide/{g['id']}"] = {
            "title": g["title"],
            "html": render_guide_content(g, base_url="")
        }

    print("Generating attributes reference page...")
    attr_html = render_attributes_page(base_url="")
    generate_static_page(
        title="Attributes Reference — Emerge",
        content_html=render_attributes_page(base_url=""),
        toc_html=render_toc_generic(),
        current_id="attributes",
        out_path=os.path.join(DOCS_DIR, "attributes.html"),
        base_url=""
    )
    docs_store["attributes"] = {
        "title": "Attributes Reference",
        "html": attr_html
    }

    print("Generating diagnostics reference page...")
    diag_html = render_diagnostics_page(base_url="")
    generate_static_page(
        title="Compiler Diagnostics & Error Codes — Emerge",
        content_html=diag_html,
        toc_html=render_toc_generic(),
        current_id="diagnostics",
        out_path=os.path.join(DOCS_DIR, "diagnostics.html"),
        base_url=""
    )
    docs_store["diagnostics"] = {
        "title": "Compiler Diagnostics & Error Codes",
        "html": diag_html
    }

    print("Generating Houdini VEX comparison page...")
    vex_topic = next((x for x in GUIDE_TOPICS if x["id"] == "vex-comparison"), None)
    if vex_topic:
        vex_html = render_guide_content(vex_topic, base_url="")
        generate_static_page(
            title="Houdini VEX vs Emerge Comparison — Emerge",
            content_html=vex_html,
            toc_html=render_toc_generic(),
            current_id="vex-comparison",
            out_path=os.path.join(DOCS_DIR, "vex-comparison.html"),
            base_url=""
        )
        docs_store["vex-comparison"] = {
            "title": "Houdini VEX vs Emerge Comparison",
            "html": vex_html
        }

    print("Generating master index.html...")
    # Default landing is guide/overview
    initial_content = docs_store["guide/overview"]["html"]
    header = render_header(base_url="")
    sidebar = render_sidebar(current_id="guide/overview", base_url="")
    
    index_html = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Emerge Language & VEX Reference — Documentation</title>
  <link rel="stylesheet" href="style.css">
  <script>
    window.EMERGE_SEARCH_INDEX = {json.dumps(search_index)};
    window.EMERGE_DOCS_STORE = {json.dumps(docs_store)};
  </script>
</head>
<body>
  {header}
  <div class="houdini-layout">
    {sidebar}
    <main class="houdini-main" id="main-content-pane">
      {initial_content}
    </main>
    <nav class="houdini-toc">
      <div class="toc-title">ON THIS PAGE</div>
      <ul class="toc-list">
        <li class="toc-item"><a class="toc-link" href="#synopsis">Synopsis</a></li>
        <li class="toc-item"><a class="toc-link" href="#arguments">Arguments</a></li>
        <li class="toc-item"><a class="toc-link" href="#returns">Returns</a></li>
        <li class="toc-item"><a class="toc-link" href="#details">Details</a></li>
        <li class="toc-item"><a class="toc-link" href="#examples">Examples</a></li>
        <li class="toc-item"><a class="toc-link" href="#see-also">See Also</a></li>
      </ul>
    </nav>
  </div>
  <script src="help.js"></script>
</body>
</html>
"""
    with open(os.path.join(DOCS_DIR, "index.html"), "w", encoding="utf-8") as f:
        f.write(index_html)

    print("All documentation generated successfully in docs/!")

if __name__ == "__main__":
    main()
