.pragma library

function withProperty(owner, name, expected) {
    const children = owner.children || [];
    for (let i = 0; i < children.length; i++) {
        if (name in children[i] && (expected === undefined || children[i][name] === expected))
            return children[i];
    }
    throw new Error("Fixture child missing property: " + name);
}
