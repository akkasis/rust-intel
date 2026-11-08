// Если интерфейс не смог стартовать — показываем ошибку вместо пустого окна.
(function () {
  function show(msg) {
    var root = document.getElementById("root");
    if (!root || root.childElementCount > 0) return;
    root.innerHTML =
      '<div style="font:14px Segoe UI,system-ui,sans-serif;color:#e4eaed;padding:32px;max-width:720px">' +
      '<h2 style="margin:0 0 8px">Rust Intelligence Desktop не смог запустить интерфейс</h2>' +
      '<p style="color:#8b989f">Отправьте этот текст разработчику:</p>' +
      '<pre style="white-space:pre-wrap;background:#171c1f;padding:12px;border-radius:8px;color:#f0525c"></pre></div>';
    root.querySelector("pre").textContent = String(msg);
  }
  window.addEventListener("error", function (e) { show((e.error && e.error.stack) || e.message); });
  window.addEventListener("unhandledrejection", function (e) { show((e.reason && e.reason.stack) || e.reason); });
  setTimeout(function () { show("Интерфейс не загрузился за 15 секунд."); }, 15000);
})();
