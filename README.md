<p align=center>
    <h1 align=center>SIGINT</h1>
</p>

## SIGINT is an Desktop/Android application for scanning and logging Bluetooth devices. It is designed for analyzing Bluetooth environments.

### Features

- Scans for nearby Bluetooth devices and logs their information.
- Provides a user-friendly interface for viewing and managing scanned devices.

## For those interested in how Rust integrates with Android, below is a brief explanation:

> the goal is to create an app that works on multiple platforms (Windows, Linux, Android) and to help others who want to achieve the same.

### How Rust works with Android?

On the Rust side, we have a library that contains the core logic for scanning and logging Bluetooth devices (using [btleplug](https://github.com/deviceplug/btleplug) and JNI bindings).

this is an example of a Rust function that can be called from Java using JNI:

```rust
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_dest4590_sigint_sniffer_Sniffer_hello(
    _env: JNIEnv,
    _class: JClass,
) {
    println!("i was called from java!");
}
```

this library is compiled into a shared object (.so) library that can be used in the Android application.

like this:

```java
public class Sniffer {
    static {
        System.loadLibrary("sigint");
    }

    public native void init();
    //                 ^---
    // then call it from the MainActivity
}
```

### How Rust builds the .so file???

We using Cargo to specify the crate type as `cdylib` and `rlib` in the `Cargo.toml` file:

```toml
[lib]
crate-type = ["cdylib", "rlib"]
```

And then we can build the .so library using cargo-ndk:

```bash
cargo install cargo-ndk
```

```bash
cargo ndk -t aarch64-linux-android build --release
```

This will generate the .so file in the `target/aarch64-linux-android/release` folder, which can be included in the Android project

I also using script to automatically put built .so file into the Android project, check [build_lib.bat](rust/build_lib.bat)

You must put the .so file in the correct folder in the Android project (e.g., `app/src/main/jniLibs/arm64-v8a/`) for it to be recognized by the application.
