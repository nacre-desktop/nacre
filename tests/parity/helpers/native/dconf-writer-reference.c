/* Developer-only GVariant wire reference for GNOME dconf's a{smv} Change blob.
 * Mirrors the public builder calls in common/dconf-changeset.c, not a runtime helper.
 */
#include <glib.h>
#include <stdio.h>
int main(void) {
    GVariantBuilder builder;
    g_variant_builder_init(&builder, G_VARIANT_TYPE("a{smv}"));
    g_variant_builder_add(&builder, "{smv}", "/org/gnome/desktop/interface/color-scheme", g_variant_new_string("prefer-dark"));
    g_variant_builder_add(&builder, "{smv}", "/org/gnome/desktop/interface/cursor-size", g_variant_new_int32(24));
    g_variant_builder_add(&builder, "{smv}", "/org/gnome/desktop/interface/cursor-theme", g_variant_new_string("breeze_cursors"));
    g_variant_builder_add(&builder, "{smv}", "/org/gnome/desktop/interface/gtk-theme", g_variant_new_string("Adwaita-dark"));
    g_variant_builder_add(&builder, "{smv}", "/org/gnome/desktop/interface/icon-theme", g_variant_new_string("Fixture Icons"));
    GVariant *value = g_variant_ref_sink(g_variant_builder_end(&builder));
    const unsigned char *bytes = g_variant_get_data(value);
    for (gsize i = 0; i < g_variant_get_size(value); i++) printf("%02x", bytes[i]);
    putchar('\n');
    g_variant_unref(value);
    return 0;
}
