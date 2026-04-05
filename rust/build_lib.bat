@echo off

cargo ndk -t aarch64-linux-android build --release

echo "Copying the built library to the Android project..."
copy target\aarch64-linux-android\release\libsigint.so C:\Users\Purpl3\IT\sigintandroid\app\src\main\java\com\jniLibs\arm64-v8a