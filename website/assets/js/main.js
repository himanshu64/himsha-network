'use strict';

const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

// ================================================
// Nav scroll effect
// ================================================
const nav = document.getElementById('nav');
if (nav) {
  const onScroll = () => nav.classList.toggle('scrolled', window.scrollY > 30);
  window.addEventListener('scroll', onScroll, { passive: true });
  onScroll();
}

// ================================================
// Hamburger menu
// ================================================
const hamburger = document.getElementById('hamburger');
const mobileMenu = document.getElementById('mobileMenu');
if (hamburger && mobileMenu) {
  const setMenu = open => {
    mobileMenu.classList.toggle('open', open);
    hamburger.setAttribute('aria-expanded', String(open));
    // Lock background scroll while the menu is open
    document.documentElement.classList.toggle('menu-open', open);
  };
  hamburger.addEventListener('click', () => setMenu(!mobileMenu.classList.contains('open')));
  document.querySelectorAll('.mobile-link, .mobile-cta a').forEach(link => {
    link.addEventListener('click', () => setMenu(false));
  });
  // Close on Escape (keyboard users) and when resizing back to desktop
  document.addEventListener('keydown', e => {
    if (e.key === 'Escape' && mobileMenu.classList.contains('open')) setMenu(false);
  });
  window.addEventListener('resize', () => {
    if (window.innerWidth > 820 && mobileMenu.classList.contains('open')) setMenu(false);
  }, { passive: true });
}

// ================================================
// SDK tabs
// ================================================
document.querySelectorAll('.sdk-tab').forEach(tab => {
  tab.addEventListener('click', () => {
    document.querySelectorAll('.sdk-tab').forEach(t => {
      t.classList.remove('active');
      t.setAttribute('aria-selected', 'false');
    });
    document.querySelectorAll('.sdk-panel').forEach(p => p.classList.remove('active'));
    tab.classList.add('active');
    tab.setAttribute('aria-selected', 'true');
    const panel = document.getElementById('panel-' + tab.dataset.tab);
    if (panel) panel.classList.add('active');
  });
});

// ================================================
// Particle field (Bitcoin-orange node network)
// ================================================
(function initParticles() {
  const canvas = document.getElementById('particleCanvas');
  if (!canvas || reduceMotion) return;
  const ctx = canvas.getContext('2d');
  const dpr = Math.min(window.devicePixelRatio || 1, 2);

  let particles = [];
  let w = 0, h = 0;

  function resize() {
    w = canvas.offsetWidth;
    h = canvas.offsetHeight;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const count = Math.min(70, Math.floor((w * h) / 16000));
    particles = Array.from({ length: count }, () => ({
      x: Math.random() * w,
      y: Math.random() * h,
      r: Math.random() * 1.6 + 0.8,
      vx: (Math.random() - 0.5) * 0.25,
      vy: (Math.random() - 0.5) * 0.25,
      a: Math.random() * 0.4 + 0.15,
    }));
  }

  resize();
  let resizeTimer;
  window.addEventListener('resize', () => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(resize, 150);
  });

  function draw() {
    ctx.clearRect(0, 0, w, h);

    for (let i = 0; i < particles.length; i++) {
      for (let j = i + 1; j < particles.length; j++) {
        const dx = particles[i].x - particles[j].x;
        const dy = particles[i].y - particles[j].y;
        const dist = Math.sqrt(dx * dx + dy * dy);
        if (dist < 130) {
          ctx.beginPath();
          ctx.strokeStyle = `rgba(247,147,26,${0.05 * (1 - dist / 130)})`;
          ctx.lineWidth = 1;
          ctx.moveTo(particles[i].x, particles[i].y);
          ctx.lineTo(particles[j].x, particles[j].y);
          ctx.stroke();
        }
      }
    }

    particles.forEach(p => {
      ctx.beginPath();
      ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
      ctx.fillStyle = `rgba(247,147,26,${p.a})`;
      ctx.fill();
      p.x += p.vx;
      p.y += p.vy;
      if (p.x < 0 || p.x > w) p.vx *= -1;
      if (p.y < 0 || p.y > h) p.vy *= -1;
    });

    requestAnimationFrame(draw);
  }
  draw();
})();

// ================================================
// Scroll reveal
// ================================================
(function initReveal() {
  const els = document.querySelectorAll('.reveal');
  if (reduceMotion || !('IntersectionObserver' in window)) {
    els.forEach(el => el.classList.add('in'));
    return;
  }
  const observer = new IntersectionObserver((entries, obs) => {
    entries.forEach(e => {
      if (e.isIntersecting) {
        e.target.classList.add('in');
        obs.unobserve(e.target);
      }
    });
  }, { threshold: 0.12, rootMargin: '0px 0px -40px 0px' });
  els.forEach(el => observer.observe(el));
})();

// ================================================
// Ask a question → open a prefilled GitHub Discussion / Issue
// ================================================
(function initAsk() {
  const form = document.getElementById('askForm');
  if (!form) return;

  const repo = form.dataset.repo;                // e.g. "your-org/himsha-network"
  const titleEl = document.getElementById('askTitle');
  const bodyEl = document.getElementById('askBody');
  const errorEl = document.getElementById('askError');
  const issueBtn = document.getElementById('askIssue');

  const clearError = () => {
    if (errorEl) errorEl.hidden = true;
    titleEl.setAttribute('aria-invalid', 'false');
  };
  titleEl.addEventListener('input', clearError);

  // Returns true when the form may be submitted (non-empty question).
  function validate() {
    if (!titleEl.value.trim()) {
      if (errorEl) errorEl.hidden = false;
      titleEl.setAttribute('aria-invalid', 'true');
      titleEl.focus();
      return false;
    }
    return true;
  }

  function buildBody() {
    const extra = bodyEl.value.trim();
    const footer = '\n\n---\n_Asked via the HIMSHA Network website._';
    return (extra ? extra + footer : footer.trimStart());
  }

  // Open a new GitHub Discussion in the Q&A category
  function openDiscussion() {
    if (!validate()) return;
    const params = new URLSearchParams({
      category: 'q-a',
      title: titleEl.value.trim(),
      body: buildBody(),
    });
    window.open(`https://github.com/${repo}/discussions/new?${params}`, '_blank', 'noopener');
  }

  // Fallback: open a labelled issue
  function openIssue() {
    if (!validate()) return;
    const params = new URLSearchParams({
      title: `[Question] ${titleEl.value.trim()}`,
      body: buildBody(),
      labels: 'question',
    });
    window.open(`https://github.com/${repo}/issues/new?${params}`, '_blank', 'noopener');
  }

  form.addEventListener('submit', e => { e.preventDefault(); openDiscussion(); });
  if (issueBtn) issueBtn.addEventListener('click', openIssue);
})();

// ================================================
// SDK tabs — sliding indicator (glides between tabs)
// ================================================
(function sdkIndicator() {
  const tabs = document.querySelector('.sdk-tabs');
  if (!tabs) return;
  const indicator = document.createElement('span');
  indicator.className = 'sdk-tab-indicator';
  tabs.appendChild(indicator);

  const tabEls = Array.from(tabs.querySelectorAll('.sdk-tab'));
  const move = tab => {
    if (!tab) return;
    indicator.style.width = tab.offsetWidth + 'px';
    indicator.style.transform = `translateX(${tab.offsetLeft}px)`;
  };
  const active = () => tabs.querySelector('.sdk-tab.active') || tabEls[0];

  tabEls.forEach(t => t.addEventListener('click', () => move(t)));
  requestAnimationFrame(() => move(active()));
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(() => move(active()));
  let rt;
  window.addEventListener('resize', () => { clearTimeout(rt); rt = setTimeout(() => move(active()), 120); }, { passive: true });
})();

// ================================================
// Hero stat count-up (first view only, rare → safe to delight)
// ================================================
(function statCountUp() {
  if (reduceMotion) return;
  const nums = Array.from(document.querySelectorAll('.stat-n')).filter(el => /^\d+$/.test(el.textContent.trim()));
  if (!nums.length || !('IntersectionObserver' in window)) return;

  const easeOut = t => 1 - Math.pow(1 - t, 3);
  function run(el) {
    const target = parseInt(el.textContent, 10);
    if (target === 0) return;                 // 0 has nothing to count
    const dur = 900;
    let start;
    function frame(ts) {
      if (start == null) start = ts;
      const p = Math.min((ts - start) / dur, 1);
      el.textContent = String(Math.round(easeOut(p) * target));
      if (p < 1) requestAnimationFrame(frame);
      else el.textContent = String(target);
    }
    requestAnimationFrame(frame);
  }

  const obs = new IntersectionObserver((entries, o) => {
    entries.forEach(e => { if (e.isIntersecting) { run(e.target); o.unobserve(e.target); } });
  }, { threshold: 0.6 });
  nums.forEach(n => obs.observe(n));
})();

// ================================================
// Scroll reading progress bar (constant motion tied to scroll position)
// ================================================
(function scrollProgress() {
  const bar = document.createElement('div');
  bar.className = 'scroll-progress';
  bar.setAttribute('aria-hidden', 'true');
  document.body.appendChild(bar);

  let ticking = false;
  function update() {
    const doc = document.documentElement;
    const max = doc.scrollHeight - doc.clientHeight;
    const p = max > 0 ? Math.min(window.scrollY / max, 1) : 0;
    bar.style.transform = `scaleX(${p})`;
    ticking = false;
  }
  window.addEventListener('scroll', () => {
    if (!ticking) { ticking = true; requestAnimationFrame(update); }
  }, { passive: true });
  update();
})();
