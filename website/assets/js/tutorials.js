'use strict';

// ---- Multi-language tabs ----
document.querySelectorAll('.tab').forEach(tab => {
  tab.addEventListener('click', () => {
    const group = tab.dataset.group;
    const target = tab.dataset.tab;

    // Deactivate all tabs in this group
    document.querySelectorAll(`.tab[data-group="${group}"]`).forEach(t => t.classList.remove('active'));
    tab.classList.add('active');

    // Show the matching panel (hide siblings)
    // Siblings are the next element's children panels
    const panelContainer = tab.closest('.tabs').nextElementSibling;
    if (panelContainer) {
      panelContainer.querySelectorAll('.tab-panel').forEach(p => p.classList.remove('active'));
      const panel = document.getElementById(target);
      if (panel) panel.classList.add('active');
    }
  });
});

// ---- Sidebar active link tracking ----
const sections = document.querySelectorAll('.tut-section');
const navLinks  = document.querySelectorAll('.tut-nav-link');

const sectionObserver = new IntersectionObserver(
  entries => {
    entries.forEach(e => {
      if (e.isIntersecting) {
        navLinks.forEach(l => l.classList.remove('active'));
        const link = document.querySelector(`.tut-nav-link[href="#${e.target.id}"]`);
        if (link) link.classList.add('active');
      }
    });
  },
  { rootMargin: '-30% 0px -60% 0px', threshold: 0 }
);

sections.forEach(s => sectionObserver.observe(s));
