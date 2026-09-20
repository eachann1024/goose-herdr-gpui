use std::{env, path::PathBuf, process::Command};

fn run(command: &mut Command) {
    let status = command.status().expect("无法运行本机构建工具");
    assert!(status.success(), "本机桥接编译失败：{command:?}");
}

fn main() {
    println!("cargo:rerun-if-env-changed=HERDR_TAILCAT_FRAMEWORK_DIR");
    for source in [
        "native/build-tailcat.sh",
        "native/TailcatBridge.m",
        "native/Notifications.m",
    ] {
        println!("cargo:rerun-if-changed={source}");
    }
    let output = Command::new("sh")
        .arg("native/build-tailcat.sh")
        .output()
        .expect("无法构建 Tailcat 桥接");
    assert!(
        output.status.success(),
        "Tailcat 桥接编译失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("target architecture");
    let arch = if arch == "aarch64" { "arm64" } else { &arch };
    run(Command::new("xcrun")
        .args([
            "clang",
            "-arch",
            arch,
            "-fobjc-arc",
            "-fmodules",
            "-c",
            "native/Notifications.m",
            "-o",
        ])
        .arg(out.join("Notifications.o")));
    run(Command::new("xcrun")
        .args(["ar", "rcs"])
        .arg(out.join("libherdr_notifications.a"))
        .arg(out.join("Notifications.o")));
    println!("cargo:rustc-link-lib=static=herdr_notifications");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=UserNotifications");
}
