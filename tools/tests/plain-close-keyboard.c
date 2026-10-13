#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>
#include "virtual-keyboard.h"

static struct zwp_virtual_keyboard_manager_v1 *manager;
static struct wl_seat *seat;

static void global(void *data, struct wl_registry *registry, uint32_t name,
                   const char *interface, uint32_t version) {
    if (strcmp(interface, "zwp_virtual_keyboard_manager_v1") == 0)
        manager = wl_registry_bind(registry, name,
            &zwp_virtual_keyboard_manager_v1_interface, 1);
    else if (strcmp(interface, "wl_seat") == 0)
        seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
}

static void removed(void *data, struct wl_registry *registry, uint32_t name) {}
static const struct wl_registry_listener listener = {global, removed};

static uint32_t milliseconds(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (uint32_t)(now.tv_sec * 1000 + now.tv_nsec / 1000000);
}

int main(void) {
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) return 2;
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &listener, NULL);
    wl_display_roundtrip(display);
    if (!manager || !seat) return 3;

    // Preserve standard evdev codes: a nested winit backend uses those codes.
    // A minimal dynamically assigned wtype keymap cannot exercise that path.
    struct xkb_context *context = xkb_context_new(XKB_CONTEXT_NO_FLAGS);
    struct xkb_rule_names names = {.layout = "us"};
    struct xkb_keymap *keymap = xkb_keymap_new_from_names(context, &names,
        XKB_KEYMAP_COMPILE_NO_FLAGS);
    if (!keymap) return 4;
    char *text = xkb_keymap_get_as_string(keymap, XKB_KEYMAP_FORMAT_TEXT_V1);
    size_t size = strlen(text) + 1;
    int fd = memfd_create("close-probe-keymap", MFD_CLOEXEC);
    if (fd < 0 || write(fd, text, size) != (ssize_t)size) return 5;
    struct zwp_virtual_keyboard_v1 *keyboard =
        zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(manager, seat);
    zwp_virtual_keyboard_v1_keymap(keyboard, WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1,
        fd, size);
    wl_display_roundtrip(display);
    close(fd);
    // Establish the parent's keyboard focus before pressing Super. Otherwise
    // the nested backend can receive Super only in wl_keyboard.enter's held
    // key list, without the press event used to track its modifier state.
    zwp_virtual_keyboard_v1_modifiers(keyboard, 0, 0, 0, 0);
    zwp_virtual_keyboard_v1_key(keyboard, milliseconds(), 42, 1);
    wl_display_roundtrip(display);
    zwp_virtual_keyboard_v1_key(keyboard, milliseconds(), 42, 0);
    wl_display_roundtrip(display);
    usleep(100000);
    uint32_t logo = 1u << xkb_keymap_mod_get_index(keymap, XKB_MOD_NAME_LOGO);
    zwp_virtual_keyboard_v1_key(keyboard, milliseconds(), 125, 1);
    zwp_virtual_keyboard_v1_modifiers(keyboard, logo, 0, 0, 0);
    wl_display_roundtrip(display);
    usleep(100000);
    zwp_virtual_keyboard_v1_key(keyboard, milliseconds(), 16, 1);
    wl_display_roundtrip(display);
    usleep(100000);
    zwp_virtual_keyboard_v1_key(keyboard, milliseconds(), 16, 0);
    zwp_virtual_keyboard_v1_key(keyboard, milliseconds(), 125, 0);
    zwp_virtual_keyboard_v1_modifiers(keyboard, 0, 0, 0, 0);
    wl_display_roundtrip(display);
    usleep(200000);
    zwp_virtual_keyboard_v1_destroy(keyboard);
    wl_display_roundtrip(display);
    free(text);
    xkb_keymap_unref(keymap);
    xkb_context_unref(context);
    wl_display_disconnect(display);
    return 0;
}
