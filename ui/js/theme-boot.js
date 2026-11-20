// Applies the last used theme before the first paint (no flash of the
// default theme). config.json stays the source of truth; this is a cache.
(function () {
  var theme = 'classic';
  try { theme = localStorage.getItem('gc-theme') || 'classic'; } catch (e) { /* storage unavailable */ }
  if (theme !== 'amber' && theme !== 'matrix' && theme !== 'dos') theme = 'classic';
  var root = document.documentElement;
  root.setAttribute('data-theme', theme);
  if (theme !== 'classic') root.classList.add('term');
  var font = 'pixel-operator';
  try { font = localStorage.getItem('gc-classic-font') || font; } catch (e) { /* storage unavailable */ }
  if (font !== 'dogica' && font !== 'geist-pixel' && font !== 'roboto-slab') font = 'pixel-operator';
  root.setAttribute('data-classic-font', font);
})();
