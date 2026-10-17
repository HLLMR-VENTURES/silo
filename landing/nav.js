/* Silo site chrome — the single source of truth for the header, footer, and theme toggle
 * across every landing page. Each page includes <script src="/nav.js"></script> and drops its
 * own header/footer markup + theme-toggle JS. Styles are scoped to `.silo-*` classes so they
 * never collide with a page's own CSS, and they read the page's design tokens (with fallbacks).
 * Theme is applied as early as possible to avoid a flash; the toggle is wired here too. */
(function () {
  "use strict";

  // Apply the saved theme immediately (pages also do this inline in <head> to beat first paint;
  // this is a harmless backstop for any page that doesn't).
  try {
    var saved = localStorage.getItem("silo-theme");
    if (saved) document.documentElement.setAttribute("data-theme", saved);
  } catch (e) {}

  var CSS = [
    ".silo-hd{position:sticky;top:0;z-index:40;background:color-mix(in srgb,var(--surface,#fcfaf4) 90%,transparent);backdrop-filter:saturate(1.4) blur(10px);-webkit-backdrop-filter:saturate(1.4) blur(10px);border-bottom:1px solid var(--border,#e4dece)}",
    ".silo-in{display:flex;align-items:center;gap:14px;max-width:var(--maxw,1180px);margin:0 auto;padding:11px 20px;flex-wrap:wrap}",
    ".silo-brand{display:flex;align-items:center;gap:10px;text-decoration:none;color:var(--text,#2a2a22)}",
    ".silo-logo{width:34px;height:34px;border-radius:9px;display:grid;place-items:center;font-weight:600;font-size:19px;color:#fff;font-family:var(--font-display,Georgia,serif);background:linear-gradient(135deg,var(--green-500,#5c8a3a),var(--green-700,#395a25));box-shadow:var(--shadow-1,0 1px 2px rgba(0,0,0,.15))}",
    ".silo-word{font-family:var(--font-display,Georgia,serif);font-weight:600;font-size:19px;color:var(--text,#2a2a22);line-height:1}",
    ".silo-sp{flex:1 1 auto}",
    ".silo-nav{display:flex;align-items:center;gap:4px;flex-wrap:wrap}",
    ".silo-nav a{color:var(--muted,#6b6a5c);text-decoration:none;font-size:14px;font-weight:600;padding:7px 11px;border-radius:var(--radius-sm,8px)}",
    ".silo-nav a:hover{color:var(--text,#2a2a22);background:color-mix(in srgb,var(--primary,#4a7330) 10%,transparent)}",
    ".silo-nav a.active{color:var(--primary,#4a7330)}",
    ".silo-cta{color:var(--on-primary,#fff)!important;background:var(--primary,#4a7330);border:1px solid var(--primary,#4a7330)}",
    ".silo-cta:hover{background:var(--primary-hover,#395a25)!important;color:var(--on-primary,#fff)!important}",
    ".silo-tog{border:1px solid var(--border,#e4dece);background:var(--raised,#fff);color:var(--text,#2a2a22);width:34px;height:34px;border-radius:var(--radius-sm,8px);font-size:15px;line-height:1;cursor:pointer}",
    ".silo-tog:hover{border-color:color-mix(in srgb,var(--primary,#4a7330) 45%,var(--border,#e4dece))}",
    ".silo-ft{border-top:1px solid var(--border,#e4dece);margin-top:48px}",
    ".silo-ft .silo-in{align-items:flex-start;gap:14px;padding-top:22px;padding-bottom:26px}",
    ".silo-ftlinks{display:flex;gap:16px;flex-wrap:wrap}",
    ".silo-ftlinks a{color:var(--muted,#6b6a5c);text-decoration:none;font-size:13px}",
    ".silo-ftlinks a:hover{color:var(--text,#2a2a22);text-decoration:underline}",
    ".silo-ftnote{flex:1 1 100%;color:var(--muted,#6b6a5c);font-size:12px;line-height:1.5;margin-top:2px}",
  ].join("");

  var HEADER =
    '<header class="silo-hd"><div class="silo-in">' +
    '<a class="silo-brand" href="/" aria-label="Silo home"><span class="silo-logo">S</span><span class="silo-word">Silo</span></a>' +
    '<span class="silo-sp"></span>' +
    '<nav class="silo-nav" aria-label="Site">' +
    '<a href="/browse/" data-p="/browse/">Browse</a>' +
    '<a href="/help/" data-p="/help/">Help</a>' +
    '<a href="/trust/" data-p="/trust/">Trust</a>' +
    '<a href="https://github.com/HLLMR/silo" rel="noopener">Source</a>' +
    '<a class="silo-cta silo-nav-cta" href="/#download">Get Silo</a>' +
    '<button class="silo-tog" id="siloTheme" type="button" aria-label="Toggle light / dark theme" title="Toggle theme">◐</button>' +
    "</nav></div></header>";

  var FOOTER =
    '<footer class="silo-ft"><div class="silo-in">' +
    '<a class="silo-brand" href="/"><span class="silo-logo">S</span><span class="silo-word">Silo</span></a>' +
    '<span class="silo-sp"></span>' +
    '<nav class="silo-ftlinks" aria-label="Footer">' +
    '<a href="/">Home</a><a href="/browse/">Browse</a><a href="/help/">Help</a>' +
    '<a href="https://github.com/HLLMR/silo/discussions" rel="noopener">Community</a>' +
    '<a href="/trust/">Trust</a><a href="/legal/">Legal</a>' +
    '<a href="https://github.com/HLLMR/silo" rel="noopener">Source</a>' +
    "</nav>" +
    '<span class="silo-ftnote">Silo indexes and verifies — it doesn\'t host mods. Aggregated from public sources · not affiliated with GIANTS Software. © HLLMR Ventures LLC.</span>' +
    "</div></footer>";

  function mount() {
    var style = document.createElement("style");
    style.textContent = CSS;
    document.head.appendChild(style);

    document.body.insertAdjacentHTML("afterbegin", HEADER);
    document.body.insertAdjacentHTML("beforeend", FOOTER);

    // Highlight the current section in the nav.
    var path = location.pathname;
    var links = document.querySelectorAll(".silo-nav a[data-p]");
    for (var i = 0; i < links.length; i++) {
      if (path.indexOf(links[i].getAttribute("data-p")) === 0) links[i].classList.add("active");
    }
    // On the home page, "Get Silo" points at the on-page #download; elsewhere, send them home to it.
    if (path === "/" || path === "/index.html") {
      var cta = document.querySelector(".silo-nav-cta");
      if (cta) cta.setAttribute("href", "#download");
    }

    var btn = document.getElementById("siloTheme");
    if (btn) {
      btn.addEventListener("click", function () {
        var cur = document.documentElement.getAttribute("data-theme");
        if (!cur) cur = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
        var next = cur === "dark" ? "light" : "dark";
        document.documentElement.setAttribute("data-theme", next);
        try {
          localStorage.setItem("silo-theme", next);
        } catch (e) {}
      });
    }
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", mount);
  else mount();
})();
