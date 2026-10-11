var apps = [
    {
        title: "Nacre Settings",
        icon: "settings",
        detail: "Set up your desktop, displays, connections and everyday preferences.",
        action: "settings:desktop",
        availability: "",
        logo: "settings"
    },
    {
        title: "Nacre Colors",
        icon: "palette",
        detail: "Explore the real colors in a wallpaper. Compare modes, preview, then apply or export.",
        action: "colors",
        availability: "",
        logo: "colors"
    },
    {
        title: "Nacre AI",
        icon: "chat_bubble",
        detail: "Your assistant workspace and ongoing conversations. Uses your existing accounts.",
        action: "ai",
        availability: "ai",
        logo: "ai"
    },
    {
        title: "Nacre Brain",
        icon: "neurology",
        detail: "Browse and search your private notes, knowledge and memories.",
        action: "brain",
        availability: "brain",
        logo: "brain"
    }
];
var help = [
    {
        title: "A panel is in the way",
        detail: "Press Escape to dismiss a launcher or wallpaper picker. Unpin the AI sidebar if you want it to close when you leave."
    },
    {
        title: "The desktop needs attention",
        detail: "Open Maintenance in Settings for health checks, updates, releases and recovery. Save your work before restarting or restoring."
    },
    {
        title: "Make the desktop comfortable",
        detail: "Settings has display scaling and a reduced-motion option. Sound, Wi-Fi and Bluetooth controls also open their detailed pages."
    },
    {
        title: "Welcome on your terms",
        detail: "Turn off Show at login below. Welcome stays in the launcher, and nacre-welcome opens it whenever you need it."
    }
];

var optional = {
    ai: {
        title: "Add Nacre AI",
        detail: "Nacre AI is optional. It connects your existing Codex or Claude installation to your work. You choose the provider and sign in with your own account.",
        setup: "In this developer build, follow ai/README.md in the Nacre project checkout for component setup. If it is already installed, the Nacre AI page in Settings manages its existing setup.",
        status: "Optional development component. Install with --with ai; provider sign-in and models remain your choice."
    },
    brain: {
        title: "Add Nacre Brain",
        detail: "Nacre Brain is optional. It gives your private notes and memories a searchable home on your computer.",
        setup: "In this developer build, follow docs/features/nacre-brain.md in the Nacre project checkout for the vault and knowledge-service setup. Keep your existing notes outside the desktop repository.",
        status: "Optional development component. Install with --with brain; Welcome never creates or moves your vault."
    }
};
