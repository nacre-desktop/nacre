/* Developer-only real GTK paint witness. Never installed with Nacre. */
#include <gtk/gtk.h>
#include <math.h>
#include <stdio.h>

static GtkWidget *button;
static char previous[8];

static void after_paint(GdkFrameClock *clock, gpointer unused) {
    (void)clock;
    (void)unused;
    GdkRGBA color;
    if (!gtk_style_context_lookup_color(gtk_widget_get_style_context(button),
                                        "theme_selected_bg_color", &color))
        return;
    char current[8];
    g_snprintf(current, sizeof current, "%02x%02x%02x",
               (unsigned)round(color.red * 255),
               (unsigned)round(color.green * 255),
               (unsigned)round(color.blue * 255));
    if (g_strcmp0(previous, current) == 0)
        return;
    g_strlcpy(previous, current, sizeof previous);
    printf("{\"consumer\":\"gtk3\",\"primary\":\"%s\","
           "\"monotonic_us\":%" G_GINT64_FORMAT ","
           "\"realtime_us\":%" G_GINT64_FORMAT ","
           "\"boundary\":\"GdkFrameClock.after-paint\"}\n",
           current, g_get_monotonic_time(), g_get_real_time());
    fflush(stdout);
}

static void style_updated(GtkWidget *widget, gpointer unused) {
    (void)unused;
    GdkRGBA color;
    if (gtk_style_context_lookup_color(gtk_widget_get_style_context(widget), "theme_selected_bg_color", &color))
        fprintf(stderr, "GTK_STYLE_UPDATED %02x%02x%02x\n", (unsigned)round(color.red*255), (unsigned)round(color.green*255), (unsigned)round(color.blue*255));
}

static void theme_changed(GObject *settings, GParamSpec *property, gpointer unused) {
    (void)property;
    (void)unused;
    char *name = NULL;
    g_object_get(settings, "gtk-theme-name", &name, NULL);
    fprintf(stderr, "GTK_THEME_NOTIFY %s\n", name ? name : "");
    g_free(name);
}

int main(int argc, char **argv) {
    gtk_init(&argc, &argv);
    g_signal_connect(gtk_settings_get_default(), "notify::gtk-theme-name", G_CALLBACK(theme_changed), NULL);
    GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(window), "Nacre palette GTK witness");
    gtk_window_set_default_size(GTK_WINDOW(window), 320, 160);
    GtkListStore *store = gtk_list_store_new(1, G_TYPE_STRING);
    GtkTreeIter row;
    gtk_list_store_append(store, &row);
    gtk_list_store_set(store, &row, 0, "Palette paint witness", -1);
    button = gtk_tree_view_new_with_model(GTK_TREE_MODEL(store));
    g_object_unref(store);
    gtk_tree_view_append_column(GTK_TREE_VIEW(button),
        gtk_tree_view_column_new_with_attributes("Palette", gtk_cell_renderer_text_new(), "text", 0, NULL));
    gtk_tree_selection_select_iter(gtk_tree_view_get_selection(GTK_TREE_VIEW(button)), &row);
    gtk_container_add(GTK_CONTAINER(window), button);
    g_signal_connect(button, "style-updated", G_CALLBACK(style_updated), NULL);
    g_signal_connect(window, "destroy", G_CALLBACK(gtk_main_quit), NULL);
    gtk_widget_show_all(window);
    g_signal_connect(gtk_widget_get_frame_clock(window), "after-paint",
                     G_CALLBACK(after_paint), NULL);
    gtk_widget_queue_draw(window);
    gtk_main();
    return 0;
}
