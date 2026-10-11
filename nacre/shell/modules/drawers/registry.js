.pragma library

function register(service, name, state, host) {
    if (!service || !name)
        return;
    service.screens = Object.assign({}, service.screens, {
        [name]: state
    });
    service.panels = Object.assign({}, service.panels, {
        [name]: host
    });
}

function release(service, name, state, host) {
    if (!service || !service.screens || !service.panels)
        return;
    if (service.screens[name] === state) {
        const next = Object.assign({}, service.screens);
        delete next[name];
        service.screens = next;
    }
    if (service.panels[name] === host) {
        const next = Object.assign({}, service.panels);
        delete next[name];
        service.panels = next;
    }
}
