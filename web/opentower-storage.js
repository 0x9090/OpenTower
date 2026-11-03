// Load this after Macroquad's mq_js_bundle.js and before calling load() for
// simtower.wasm. It supplies the target-specific imports used by the Rust
// localStorage persistence adapter.
(function () {
    "use strict";

    const encoder = new TextEncoder();

    function rustString(pointer, length) {
        return UTF8ToString(pointer, length);
    }

    function storedBytes(keyPointer, keyLength) {
        const value = window.localStorage.getItem(rustString(keyPointer, keyLength));
        return value === null ? null : encoder.encode(value);
    }

    const plugin = {
        name: "opentower_storage",
        version: 1,
        register_plugin(importObject) {
            importObject.env.opentower_storage_value_len = function (keyPointer, keyLength) {
                try {
                    const bytes = storedBytes(keyPointer, keyLength);
                    return bytes === null ? -1 : bytes.length;
                } catch (error) {
                    console.error("OpenTower could not read localStorage", error);
                    return -2;
                }
            };

            importObject.env.opentower_storage_read = function (
                keyPointer,
                keyLength,
                outputPointer,
                outputLength
            ) {
                try {
                    const bytes = storedBytes(keyPointer, keyLength);
                    if (bytes === null) {
                        return -1;
                    }
                    if (bytes.length > outputLength) {
                        return -2;
                    }
                    new Uint8Array(wasm_memory.buffer, outputPointer, bytes.length).set(bytes);
                    return bytes.length;
                } catch (error) {
                    console.error("OpenTower could not read localStorage", error);
                    return -2;
                }
            };

            importObject.env.opentower_storage_write = function (
                keyPointer,
                keyLength,
                valuePointer,
                valueLength
            ) {
                try {
                    const key = rustString(keyPointer, keyLength);
                    const value = rustString(valuePointer, valueLength);
                    window.localStorage.setItem(key, value);
                    return 1;
                } catch (error) {
                    console.error("OpenTower could not write localStorage", error);
                    return 0;
                }
            };
        },
    };

    miniquad_add_plugin(plugin);
})();
