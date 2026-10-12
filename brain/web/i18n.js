"use strict";
(() => {
  const accents = {
    a: "á",
    b: "ƀ",
    c: "ç",
    d: "ď",
    e: "ë",
    f: "ƒ",
    g: "ğ",
    h: "ħ",
    i: "ï",
    j: "ĵ",
    k: "ķ",
    l: "ľ",
    m: "ḿ",
    n: "ñ",
    o: "ö",
    p: "þ",
    q: "ɋ",
    r: "ŕ",
    s: "š",
    t: "ţ",
    u: "ü",
    v: "ṽ",
    w: "ŵ",
    x: "ẋ",
    y: "ÿ",
    z: "ž",
  };
  const requested = document.documentElement.dataset.nacreLanguage || "system";
  const language = requested === "pseudo" ? "pseudo" : "en";
  const locale =
    requested === "system" || requested === "pseudo" ? undefined : requested;
  function pseudo(text) {
    let length = 0;
    const value = text.replace(
      /<[^>]+>|\{[A-Za-z_][A-Za-z0-9_]*\}|[A-Za-z]/g,
      (token) => {
        if (token.startsWith("<") || token.startsWith("{")) return token;
        length++;
        const char = accents[token.toLowerCase()] || token;
        return token === token.toUpperCase() ? char.toUpperCase() : char;
      },
    );
    return "[" + value + " " + "~".repeat(Math.ceil(length * 0.4)) + "]";
  }
  function tr(id, fallback, args = {}) {
    let value = window.NacreEnglish[id] ?? fallback;
    if (value && typeof value === "object")
      value =
        value[new Intl.PluralRules(locale).select(args.count)] ?? value.other;
    if (language === "pseudo") value = pseudo(value);
    return value.replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (match, key) =>
      Object.prototype.hasOwnProperty.call(args, key)
        ? typeof args[key] === "number"
          ? new Intl.NumberFormat(locale).format(args[key])
          : String(args[key])
        : match,
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
