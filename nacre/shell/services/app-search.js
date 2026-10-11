.pragma library

function normalize(value) {
    const text = String(value || "").toLowerCase();
    return (typeof text.normalize === "function" ? text.normalize("NFKD") : text).replace(/[\u0300-\u036f]/g, "");
}
function score(value, query) {
    if (!value || !query)
        return 0;
    if (value === query)
        return 1000;
    if (value.startsWith(query))
        return 900 - Math.min(100, value.length - query.length);
    const words = value.split(/[^a-z0-9\u0080-\uffff]+/).filter(Boolean);
    if (words.some(word => word.startsWith(query)))
        return 750;
    if (query.length > 1 && words.map(word => word[0]).join("").startsWith(query))
        return 680;
    const contained = value.indexOf(query);
    if (contained >= 0)
        return 600 - Math.min(contained, 100);
    let previous = -1, gaps = 0, first = -1;
    for (const letter of query) {
        const position = value.indexOf(letter, previous + 1);
        if (position < 0)
            return 0;
        if (first < 0)
            first = position;
        else
            gaps += position - previous - 1;
        previous = position;
    }
    return Math.max(1, 250 - first - Math.min(gaps, 200));
}
function prepare(entry) {
    return {
        entry: entry,
        fields: [normalize(entry.name), normalize(entry.genericName), normalize(entry.comment), normalize([...(entry.keywords || [])].join(" ")), normalize(entry.id)]
    };
}
function query(records, value) {
    const search = normalize(value).trim().slice(0, 256);
    if (!search)
        return records.map(record => record.entry);
    const words = search.split(/\s+/);
    const ranked = [];
    for (const record of records) {
        let total = 0, valid = true;
        for (const word of words) {
            const best = Math.max(...record.fields.map((field, index) => score(field, word) * (index === 0 ? 1 : .7)));
            if (!best) {
                valid = false;
                break;
            }
            total += best;
        }
        if (valid)
            ranked.push({
                entry: record.entry,
                score: total + score(record.fields[0], search)
            });
    }
    ranked.sort((left, right) => right.score - left.score || left.entry.name.localeCompare(right.entry.name) || left.entry.id.localeCompare(right.entry.id));
    return ranked.map(result => result.entry);
}
