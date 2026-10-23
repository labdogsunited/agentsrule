// Emerge Language Documentation Interactive Client
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
      const hash = window.location.hash.replace('#', '') || 'guide/using-overview';
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
          // The right-hand list follows the page: its own headings.
          const toc = document.querySelector('.houdini-toc .toc-list');
          if (toc) {
            toc.textContent = '';
            mainEl.querySelectorAll('h2').forEach((h, i) => {
              if (!h.id) h.id = 'sec-' + (h.textContent || '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') + '-' + i;
              const li = document.createElement('li'), a = document.createElement('a');
              li.className = 'toc-item'; a.className = 'toc-link'; a.textContent = h.textContent;
              a.href = '#' + hash; a.addEventListener('click', (e) => { e.preventDefault(); h.scrollIntoView({ behavior: 'smooth', block: 'start' }); });
              li.appendChild(a); toc.appendChild(li);
            });
          }
          window.scrollTo(0, 0);
        }
      }
    }
    window.addEventListener('hashchange', loadPageFromHash);
    loadPageFromHash();
  }
});
