(function () {
  var pref = localStorage.getItem("theme");
  if (pref !== "light" && pref !== "dark") pref = "system";
  var dark = pref === "dark" || (pref !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
  var theme = dark ? "dark" : "light";
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
})();
