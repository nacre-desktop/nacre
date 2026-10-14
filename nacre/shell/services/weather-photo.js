.pragma library

function plain(value) {
    return String(value || "").replace(/<[^>]*>/g, "").replace(/&(?:amp|quot|apos|lt|gt|nbsp);/g, token => ({
                "&amp;": "&",
                "&quot;": '"',
                "&apos;": "'",
                "&lt;": "<",
                "&gt;": ">",
                "&nbsp;": " "
            })[token]).replace(/[\u0000-\u001f\u007f]/g, " ").trim();
}
function imageUrl(url) {
    if (typeof url !== "string" || url.length > 2048 || !/^https:\/\/upload\.wikimedia\.org\/wikipedia\/commons\/[A-Za-z0-9_%()/!~*'.,+:-]+$/.test(url))
        return false;
    try {
        return !/(?:\.\.|[\\\u0000-\u001f\u007f])/.test(decodeURIComponent(url));
    } catch (error) {
        return false;
    }
}
function sourceUrl(url) {
    return typeof url === "string" && url.length < 2048 && /^https:\/\/commons\.wikimedia\.org\/wiki\/File(?::|%3[Aa])[^?#\s]+$/.test(url);
}
function cityImage(data, latitude, longitude) {
    const page = data?.query?.pages?.[0];
    const title = page?.pageimage, point = page?.coordinates?.[0];
    if (!Number.isFinite(latitude) || !Number.isFinite(longitude) || Math.abs(latitude) > 90 || Math.abs(longitude) > 180 || !point || !Number.isFinite(point.lat) || !Number.isFinite(point.lon) || Math.abs(point.lat) > 90 || Math.abs(point.lon) > 180 || point.globe !== "earth")
        return "";
    const dx = ((point.lon - longitude + 540) % 360 - 180) * Math.cos((point.lat + latitude) * Math.PI / 360), dy = point.lat - latitude;
    if (Math.sqrt(dx * dx + dy * dy) * 111.2 > 50)
        return "";
    return page?.ns === 0 && typeof title === "string" && title.length < 240 && /\.(?:jpe?g|png)$/i.test(title) ? title : "";
}
function metadata(data) {
    const info = data?.query?.pages?.[0]?.imageinfo?.[0];
    if (!info || !["image/jpeg", "image/png"].includes(info.mime))
        return null;
    const remote = info.thumburl || info.url, url = typeof remote === "string" ? remote.split("?")[0] : "", width = info.thumbwidth || info.width, height = info.thumbheight || info.height;
    if (!imageUrl(url) || !sourceUrl(info.descriptionurl) || !Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0 || width > 1400 || height > 1400 || width * height > 1960000)
        return null;
    const meta = info.extmetadata || {}, author = plain(meta.Attribution?.value || meta.Artist?.value), license = plain(meta.LicenseShortName?.value);
    // Restrict reuse to familiar free licenses; display credit beside the image.
    if (!author || author.length > 160 || !license || license.length > 40 || !/^(?:CC BY(?:-SA)? (?:1\.0|2\.0|2\.5|3\.0|4\.0)|CC0(?: 1\.0)?|Public domain)$/i.test(license))
        return null;
    return {
        url: url,
        source: info.descriptionurl,
        author: author,
        license: license
    };
}
function cached(data, city, now, target) {
    return data?.city === city && data.target === target && Number.isFinite(data.checked) && data.checked <= now + 60 && now - data.checked < 604800 && imageUrl(data.url) && sourceUrl(data.source) && typeof data.author === "string" && data.author.length <= 160 && typeof data.license === "string" && data.license.length <= 40;
}
