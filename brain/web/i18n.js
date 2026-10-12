"use strict";
(() => {
  const requested = document.documentElement.dataset.nacreLanguage || "system";
  const language = requested === "pseudo" ? "pseudo" : "en";
  const locale =
    requested === "system" || requested === "pseudo" ? undefined : requested;
  function tr(id, fallback, args = {}) {
    return window.NacreTranslation.translate(
      window.NacreEnglish,
      language,
      id,
      fallback,
      args,
      (value) => new Intl.NumberFormat(locale).format(value),
    );
  }
  window.NacreI18n = {
    tr,
    number: (value) => new Intl.NumberFormat(locale).format(value),
    date: (value) => new Intl.DateTimeFormat(locale).format(value),
  };
  for (const element of document.querySelectorAll("[data-i18n-node]")) {
    const node = Array.from(element.childNodes).find(
      (child) => child.nodeType === Node.TEXT_NODE && child.nodeValue.trim(),
    );
    if (node)
      node.nodeValue = node.nodeValue.replace(
        node.nodeValue.trim(),
        tr(element.dataset.i18nNode, node.nodeValue.trim()),
      );
  }
  for (const attribute of ["title", "placeholder", "aria-label"])
    for (const element of document.querySelectorAll(
      "[data-i18n-" + attribute + "]",
    ))
      element.setAttribute(
        attribute,
        tr(
          element.getAttribute("data-i18n-" + attribute),
          element.getAttribute(attribute),
        ),
      );
})();
