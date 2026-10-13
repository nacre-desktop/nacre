"use strict";

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
function pseudo(text) {
  let length = 0;
  const result = text.replace(
    /<[^>]+>|\{[A-Za-z_][A-Za-z0-9_]*\}|[A-Za-z]/g,
    (token) => {
      if (token.startsWith("{") || token.startsWith("<")) return token;
      length++;
      const accented = accents[token.toLowerCase()] || token;
      return token === token.toUpperCase() ? accented.toUpperCase() : accented;
    },
  );
  return "[" + result + " " + "~".repeat(Math.ceil(length * 0.4)) + "]";
}
function format(text, args, numberFormatter) {
  return text.replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (match, key) =>
    Object.prototype.hasOwnProperty.call(args, key)
      ? numberFormatter && typeof args[key] === "number"
        ? numberFormatter(args[key])
        : String(args[key])
      : match,
  );
}
function translate(messages, language, id, fallback, args, numberFormatter) {
  args = args || {};
  let message = messages[id] === undefined ? fallback : messages[id];
  if (message && typeof message === "object")
    message =
      Number(args.count) === 1 ? (message.one ?? message.other) : message.other;
  if (typeof message !== "string")
    message = typeof fallback === "string" ? fallback : fallback?.other || "";
  return format(
    language === "pseudo" ? pseudo(message) : message,
    args,
    numberFormatter,
  );
}
function localize(value, tr) {
  if (Array.isArray(value)) return value.map((row) => localize(row, tr));
  if (!value || typeof value !== "object") return value;
  const result = {};
  for (const key of Object.keys(value))
    result[key] = key.endsWith("Id")
      ? value[key]
      : value[key + "Id"]
        ? tr(value[key + "Id"], value[key])
        : localize(value[key], tr);
  return result;
}

window.NacreTranslation = { translate, pseudo, format };
