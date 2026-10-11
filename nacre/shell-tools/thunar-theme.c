/* App-scoped GTK3 palette reload. No polling or application restart. */
#include <gtk/gtk.h>
#include <gmodule.h>

static GtkCssProvider *provider;
static guint pending;
static gchar *palette_path;
static gchar *settings_path;
static gchar *style_path;
static GFileMonitor *monitor;

static gboolean reload_style(gpointer unused) {
    (void) unused;
    pending = 0;
    gchar *palette = NULL, *style = NULL;
    if (!g_file_get_contents(palette_path, &palette, NULL, NULL)) return G_SOURCE_REMOVE;
    if (!g_file_get_contents(style_path, &style, NULL, NULL)) {
        g_free(palette);
        return G_SOURCE_REMOVE;
    }
    gchar *css = g_strconcat(palette, "\n", style, NULL);
    GError *error = NULL;
    if (!gtk_css_provider_load_from_data(provider, css, -1, &error)) {
        g_warning("Nacre Thunar style: %s", error->message);
        g_clear_error(&error);
    }
    GKeyFile *settings = g_key_file_new();
    if (g_key_file_load_from_file(settings, settings_path, G_KEY_FILE_NONE, NULL)) {
        gchar *icons = g_key_file_get_string(settings, "Settings", "gtk-icon-theme-name", NULL);
        if (icons) g_object_set(gtk_settings_get_default(), "gtk-icon-theme-name", icons, NULL);
        g_free(icons);
        gchar *theme = g_key_file_get_string(settings, "Settings", "gtk-theme-name", NULL);
        if (theme) g_object_set(gtk_settings_get_default(), "gtk-theme-name", theme, NULL);
        g_free(theme);
        if (g_key_file_has_key(settings, "Settings", "gtk-application-prefer-dark-theme", NULL)) {
            gboolean dark = g_key_file_get_integer(settings, "Settings", "gtk-application-prefer-dark-theme", NULL) != 0;
            g_object_set(gtk_settings_get_default(), "gtk-application-prefer-dark-theme", dark, NULL);
        }
    }
    g_key_file_unref(settings);
    g_free(css); g_free(palette); g_free(style);
    return G_SOURCE_REMOVE;
}

static void changed(GFileMonitor *m, GFile *file, GFile *other,
                    GFileMonitorEvent event, gpointer unused) {
    (void)m; (void)event; (void)unused;
    gchar *path = g_file_get_path(file);
    gchar *destination = other ? g_file_get_path(other) : NULL;
    if (g_strcmp0(path, palette_path) == 0 || g_strcmp0(path, settings_path) == 0 ||
        g_strcmp0(destination, palette_path) == 0 || g_strcmp0(destination, settings_path) == 0) {
        if (pending) g_source_remove(pending);
        pending = g_timeout_add(60, reload_style, NULL);
    }
    g_free(path);
    g_free(destination);
}

G_MODULE_EXPORT void gtk_module_init(gint *argc, gchar ***argv) {
    (void)argc; (void)argv;
    const gchar *style = g_getenv("NACRE_THUNAR_STYLE");
    if (!style || g_strcmp0(g_get_prgname(), "thunar") != 0) return;
    style_path = g_strdup(style);
    gchar *directory = g_build_filename(g_get_user_config_dir(), "gtk-3.0", NULL);
    palette_path = g_build_filename(directory, "gtk.css", NULL);
    settings_path = g_build_filename(directory, "settings.ini", NULL);
    provider = gtk_css_provider_new();
    gtk_style_context_add_provider_for_screen(gdk_screen_get_default(),
        GTK_STYLE_PROVIDER(provider), GTK_STYLE_PROVIDER_PRIORITY_USER + 1);
    reload_style(NULL);
    GFile *folder = g_file_new_for_path(directory);
    monitor = g_file_monitor_directory(folder, G_FILE_MONITOR_WATCH_MOVES, NULL, NULL);
    if (monitor) g_signal_connect(monitor, "changed", G_CALLBACK(changed), NULL);
    g_object_unref(folder);
    g_free(directory);
}
