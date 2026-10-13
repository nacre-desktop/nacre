var apps = [
    {
        titleId: "welcome.catalog.title.nacre-settings",
        title: "Nacre Settings",
        icon: "settings",
        detailId: "welcome.catalog.detail.set-up-your-desktop-displays-connections-and-everyday-preferences",
        detail: "Set up your desktop, displays, connections and everyday preferences.",
        action: "settings:desktop",
        availability: "",
        logo: "settings"
    },
    {
        titleId: "welcome.catalog.title.nacre-colors",
        title: "Nacre Colors",
        icon: "palette",
        detailId: "welcome.catalog.detail.explore-the-real-colors-in-a-wallpaper-compare-modes-preview-then",
        detail: "Explore the real colors in a wallpaper. Compare modes, preview, then apply or export.",
        action: "colors",
        availability: "",
        logo: "colors"
    },
    {
        titleId: "welcome.catalog.title.nacre-ai",
        title: "Nacre AI",
        icon: "chat_bubble",
        detailId: "welcome.catalog.detail.your-assistant-workspace-and-ongoing-conversations-uses-your-exis",
        detail: "Your assistant workspace and ongoing conversations. Uses your existing accounts.",
        action: "ai",
        availability: "ai",
        logo: "ai"
    },
    {
        titleId: "welcome.catalog.title.nacre-brain",
        title: "Nacre Brain",
        icon: "neurology",
        detailId: "welcome.catalog.detail.browse-and-search-your-private-notes-knowledge-and-memories",
        detail: "Browse and search your private notes, knowledge and memories.",
        action: "brain",
        availability: "brain",
        logo: "brain"
    }
];
var help = [
    {
        titleId: "welcome.catalog.title.a-panel-is-in-the-way",
        title: "A panel is in the way",
        detailId: "welcome.catalog.detail.press-escape-to-dismiss-a-launcher-or-wallpaper-picker-unpin-the",
        detail: "Press Escape to dismiss a launcher or wallpaper picker. Unpin the AI sidebar if you want it to close when you leave."
    },
    {
        titleId: "welcome.catalog.title.the-desktop-needs-attention",
        title: "The desktop needs attention",
        detailId: "welcome.catalog.detail.open-maintenance-in-settings-for-health-checks-updates-releases-a",
        detail: "Open Maintenance in Settings for health checks, updates, releases and recovery. Save your work before restarting or restoring."
    },
    {
        titleId: "welcome.catalog.title.make-the-desktop-comfortable",
        title: "Make the desktop comfortable",
        detailId: "welcome.catalog.detail.settings-has-display-scaling-and-a-reduced-motion-option-sound-wi",
        detail: "Settings has display scaling and a reduced-motion option. Sound, Wi-Fi and Bluetooth controls also open their detailed pages."
    },
    {
        titleId: "welcome.catalog.title.welcome-on-your-terms",
        title: "Welcome on your terms",
        detailId: "welcome.catalog.detail.turn-off-show-at-login-below-welcome-stays-in-the-launcher-and-na",
        detail: "Turn off Show at login below. Welcome stays in the launcher, and nacre-welcome opens it whenever you need it."
    }
];

var optional = {
    ai: {
        titleId: "welcome.catalog.title.add-nacre-ai",
        title: "Add Nacre AI",
        detailId: "welcome.catalog.detail.nacre-ai-is-optional-it-connects-your-existing-codex-or-claude-in",
        detail: "Nacre AI is optional. It connects your existing Codex or Claude installation to your work. You choose the provider and sign in with your own account.",
        setupId: "welcome.catalog.setup.in-this-developer-build-follow-ai-readme-md-in-the-nacre-project",
        setup: "In this developer build, follow ai/README.md in the Nacre project checkout for component setup. If it is already installed, the Nacre AI page in Settings manages its existing setup.",
        statusId: "welcome.catalog.status.optional-development-component-install-with-with-ai-provider-sign",
        status: "Optional development component. Install with --with ai; provider sign-in and models remain your choice."
    },
    brain: {
        titleId: "welcome.catalog.title.add-nacre-brain",
        title: "Add Nacre Brain",
        detailId: "welcome.catalog.detail.nacre-brain-is-optional-it-gives-your-private-notes-and-memories",
        detail: "Nacre Brain is optional. It gives your private notes and memories a searchable home on your computer.",
        setupId: "welcome.catalog.setup.in-this-developer-build-follow-docs-features-nacre-brain-md-in-th",
        setup: "In this developer build, follow docs/features/nacre-brain.md in the Nacre project checkout for the vault and knowledge-service setup. Keep your existing notes outside the desktop repository.",
        statusId: "welcome.catalog.status.optional-development-component-install-with-with-brain-welcome-ne",
        status: "Optional development component. Install with --with brain; Welcome never creates or moves your vault."
    }
};

function localize(value, translate) {
    if (Array.isArray(value))
        return value.map(row => localize(row, translate));
    if (!value || typeof value !== "object")
        return value;
    const result = {};
    for (const key of Object.keys(value))
        result[key] = !key.endsWith("Id") && value[key + "Id"] && translate ? translate(value[key + "Id"], value[key]) : value[key];
    return result;
}
