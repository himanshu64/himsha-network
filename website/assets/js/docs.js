'use strict';

// ================================================
// Docs page — TOC toggle (mobile) + scroll spy
// ================================================
(function () {
  const sidebar = document.getElementById('docSidebar');
  const toggle = document.getElementById('tocToggle');

  if (toggle && sidebar) {
    toggle.addEventListener('click', () => {
      const open = sidebar.classList.toggle('open');
      toggle.setAttribute('aria-expanded', String(open));
    });
  }

  const links = Array.from(document.querySelectorAll('.doc-toc-link'));
  if (!links.length) return;

  // Map each link to its target heading
  const targets = links
    .map(link => {
      const id = decodeURIComponent(link.getAttribute('href').slice(1));
      const el = document.getElementById(id);
      return el ? { link, el } : null;
    })
    .filter(Boolean);

  // Close mobile sidebar after picking a section
  links.forEach(link => {
    link.addEventListener('click', () => {
      if (sidebar && window.innerWidth <= 920) {
        sidebar.classList.remove('open');
        if (toggle) toggle.setAttribute('aria-expanded', 'false');
      }
    });
  });

  if (!('IntersectionObserver' in window)) return;

  const visible = new Set();
  const setActive = () => {
    let current = null;
    // Choose the topmost visible heading
    for (const t of targets) {
      if (visible.has(t.el)) { current = current || t; }
    }
    if (!current) return;
    links.forEach(l => l.classList.remove('active'));
    current.link.classList.add('active');
  };

  const observer = new IntersectionObserver(entries => {
    entries.forEach(e => {
      if (e.isIntersecting) visible.add(e.target);
      else visible.delete(e.target);
    });
    setActive();
  }, { rootMargin: '-80px 0px -70% 0px', threshold: 0 });

  targets.forEach(t => observer.observe(t.el));
})();
