#include <gtk/gtk.h>
#include <glib/gstdio.h>
static GtkWidget *window;
static const char *path;
static int phase;
static gboolean check_palette(gpointer unused) {
    (void)unused;
    GdkRGBA color;
    gtk_style_context_get_color(gtk_widget_get_style_context(window), GTK_STATE_FLAG_NORMAL, &color);
    if (!phase) {
        if (color.red < 0.95 || color.green > 0.05) g_error("Initial palette was not applied");
        gchar *next = g_strconcat(path, ".new", NULL);
        gchar *css = NULL;
        g_file_get_contents(path, &css, NULL, NULL);
        gchar **parts = g_strsplit(css, "#ff0000", -1);
        gchar *updated = g_strjoinv("#00ff00", parts);
        g_file_set_contents(next, updated, -1, NULL);
        g_rename(next, path);
        g_strfreev(parts); g_free(css); g_free(updated); g_free(next);
        phase = 1;
        return G_SOURCE_CONTINUE;
    }
    if (color.green < 0.95 || color.red > 0.05) g_error("Atomic palette replacement did not update the live widget");
    g_print("Initial and live replacement colors passed in the same GTK process\n");
    gtk_main_quit();
    return G_SOURCE_REMOVE;
}
int main(int argc, char **argv) {
    path = argv[1];
    g_set_prgname("thunar");
    gtk_init(&argc, &argv);
    window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_style_context_add_class(gtk_widget_get_style_context(window), "thunar");
    gtk_widget_realize(window);
    g_timeout_add(400, check_palette, NULL);
    gtk_main();
    gtk_widget_destroy(window);
    return 0;
}
