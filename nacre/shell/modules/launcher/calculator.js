.pragma library

// A small arithmetic grammar, never JavaScript or shell evaluation.
function evaluate(source) {
    const text = String(source).trim();
    if (!text)
        return {
            ok: false,
            error: "Type a calculation, for example (12 + 8) / 5"
        };
    if (text.length > 256)
        return {
            ok: false,
            error: "Keep calculations under 256 characters"
        };
    let cursor = 0, depth = 0, operations = 0;
    function skip() {
        while (/\s/.test(text[cursor] || "") && cursor < text.length)
            cursor++;
    }
    function consume(token) {
        skip();
        if (text.slice(cursor, cursor + token.length) !== token)
            return false;
        cursor += token.length;
        return true;
    }
    function expression() {
        let result = product();
        while (true) {
            if (consume("+"))
                result += product();
            else if (consume("-"))
                result -= product();
            else
                return result;
            if (++operations > 256)
                throw Error("Calculation is too long");
        }
    }
    function product() {
        let result = unary();
        while (true) {
            skip();
            if (text.slice(cursor, cursor + 2) === "**")
                return result;
            if (consume("*"))
                result *= unary();
            else if (consume("/"))
                result /= unary();
            else if (consume("%"))
                result %= unary();
            else
                return result;
            if (++operations > 256)
                throw Error("Calculation is too long");
        }
    }
    function unary() {
        if (++depth > 32)
            throw Error("Too many nested operations");
        let result;
        if (consume("+"))
            result = unary();
        else if (consume("-"))
            result = -unary();
        else
            result = power();
        depth--;
        return result;
    }
    function power() {
        const result = primary();
        return consume("**") || consume("^") ? Math.pow(result, unary()) : result;
    }
    function primary() {
        skip();
        if (consume("(")) {
            const value = expression();
            if (!consume(")"))
                throw Error("Close the parenthesis");
            return value;
        }
        const number = /^(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?/i.exec(text.slice(cursor));
        if (number) {
            cursor += number[0].length;
            return Number(number[0]);
        }
        const name = /^[a-z][a-z0-9]*/i.exec(text.slice(cursor));
        if (name) {
            cursor += name[0].length;
            const key = name[0].toLowerCase();
            if (key === "pi")
                return Math.PI;
            if (key === "e")
                return Math.E;
            if (!["sqrt", "abs", "sin", "cos", "tan", "log", "log10", "floor", "ceil", "round"].includes(key))
                throw Error("Unknown function or constant");
            if (!consume("("))
                throw Error("Put the function's argument in parentheses");
            const value = expression();
            if (!consume(")"))
                throw Error("Close the parenthesis");
            return Math[key](value);
        }
        throw Error("Use numbers, arithmetic operators or supported functions");
    }
    try {
        const value = expression();
        skip();
        if (cursor !== text.length)
            throw Error("Check the expression near " + text.slice(cursor, cursor + 12));
        if (!Number.isFinite(value))
            throw Error("The result is undefined or outside the numeric range");
        return {
            ok: true,
            value: String(Number(value.toPrecision(14))),
            error: ""
        };
    } catch (error) {
        return {
            ok: false,
            error: error.message
        };
    }
}
