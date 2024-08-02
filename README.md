# MMTk ART Binding

This repository allows [MMTk](https://mmtk.io) to work with the [Android Runtime](https://android.googlesource.com/platform/art) (ART).

This repository hosts the MMTk-side of the [binding](https://docs.mmtk.io/portingguide/portability.html).
Our [ART fork](https://github.com/k-sareen/art/tree/mmtk-art-rebase) hosts the ART-side of the binding as well as any changes required to ART to allow MMTk to work.
Our [MMTk fork](https://github.com/k-sareen/mmtk-core/tree/main-art-rebase) hosts changes to MMTk that are required to allow MMTk to work with ART.

## Current Status

We only support the `StickyImmix` and `Immix` plans in MMTk core.
This is because we currently require roots to be pinned during a GC as there is an impedence mismatch between MMTk's and ART's expectations[^1].

We can run headless ART builds both on target and host and for both x86_64 and aarch64 devices.

We have allocation fast-paths implemented for all architectures.

We can boot an AOSP build on an x86_64 Cuttlefish VM with the `StickyImmix` plan.

[^1]: Namely, ART expects a root visitor to process the root immediately while MMTk expects a list of root slots to seed the transitive closure.

## Building and Installation Instructions

### Setting up

Since we had to add/change repositories to get MMTk to build with ART, we maintain our own `repo` manifest file.
It is based off the `master-art` manifest file.
We lock all versions of dependencies to ensure we can always build MMTk.
The ART fork is based on commit [`451cfcf9d09515ef60d76bd8551fc68c6e3bf621`](https://android.googlesource.com/platform/art/+/451cfcf9d09515ef60d76bd8551fc68c6e3bf621).

```shell
$ mkdir android-mmtk
$ cd android-mmtk
$ repo init -u https://github.com/k-sareen/mmtk-art-manifest -b mmtk-art
```

### Building

#### Building Headless ART

Set up the environment and build target.
```shell
$ source build/envsetup.sh
$ export VARIANT="eng"
$ lunch silvermont-trunk_staging-${VARIANT}  # For Linux x86_64 host builds
```
OR for aarch64 device target:
```shell
$ lunch armv8-trunk_staging-${VARIANT}       # For device target builds
```

If you want a release/performance build set the `VARIANT` environment variable to `userdebug` or `user`.

> **Note:** If you want MMTk to build in release mode as well, then you will have to comment out the debug build flags in `mmtk-core/Android.bp` and uncomment the release build flags in `mmtk-core/Android.bp` as well as `mmtk-art/Android.bp`.

To build MMTk ART with `Immix`:
```shell
$ RUST_BACKTRACE=1 ART_USE_MMTK=true ART_USE_READ_BARRIER=false ART_USE_WRITE_BARRIER=false ART_DEFAULT_GC_TYPE=SS ./art/tools/buildbot-build.sh --{host,target} --installclean --skip-run-tests-build
```
Note you may have to change the default GC inside MMTk core to select `Immix` by default.

To build MMTk ART with `StickyImmix`:
```shell
$ RUST_BACKTRACE=1 ART_USE_MMTK=true ART_USE_READ_BARRIER=false ART_USE_WRITE_BARRIER=true ART_DEFAULT_GC_TYPE=SS ./art/tools/buildbot-build.sh --{host,target} --installclean --skip-run-tests-build
```

#### Building ART APEX

Set up the environment and build target.
```shell
$ source build/envsetup.sh
$ export VARIANT="eng"
$ banchan com.android.art ${VARIANT} x86_64    # For x86_64 target builds
```
See [above](#building-headless-art) if you want a release/performance build.

To build MMTk ART with `Immix` (note this is not thoroughly tested):
```shell
$ RUST_BACKTRACE=1 ART_USE_MMTK=1 ART_USE_READ_BARRIER=false ART_USE_WRITE_BARRIER=false ART_DEFAULT_GC_TYPE=SS m apps_only dist
```

To build MMTk ART with `StickyImmix`:
```shell
$ RUST_BACKTRACE=1 ART_USE_MMTK=1 ART_USE_READ_BARRIER=false ART_USE_WRITE_BARRIER=true ART_DEFAULT_GC_TYPE=SS m apps_only dist
```

### Installation

#### Cuttlefish VM

We use Cuttlefish images from [this Android CI build](https://ci.android.com/builds/submitted/11379769/aosp_cf_x86_64_phone-trunk_staging-userdebug/latest).

We run the Cuttlefish VM like so:
```shell
$ HOME=$PWD taskset -c 0-3 ./bin/launch_cvd -report_anonymous_usage_stats=n --daemon --gpu_mode=guest_swiftshader -vm_manager=qemu_cli -guest_enforce_security=false -cpus=4
```

QEMU was used since `crosvm` was crashing on our development machines.
It is likely MMTk works with `crosvm` but it is untested.

We disable SELinux due to reasons mentioned [below](#known-limitations).

Install ART with `adb` like so:
```shell
$ adb install /path/to/android/root/out/dist/com.android.art.apex
$ adb reboot
```

If everything is fine then you should boot into the Android lock screen.

#### Running

You can check if you are using MMTk if it is printing logs in logcat like so:
```
$ adb logcat | grep "mmtk-art"
[...]
08-01 12:18:58.734  1025  1251 I mmtk-art64: mmtk::util::heap::gc_trigger: [POLL] immix: Triggering collection (7722/7721 pages)
08-01 12:18:58.736  1025  1039 I mmtk-art64: mmtk::plan::sticky::immix::global: Full heap GC
08-01 12:18:58.818  1025  1039 I mmtk-art64: mmtk::scheduler::scheduler: End of GC (7328/7991 pages, took 81 ms)
08-01 12:22:49.812  1025  1063 I mmtk-art64: mmtk::util::heap::gc_trigger: [POLL] immix: Triggering collection (7994/7991 pages)
08-01 12:22:49.814  1025  1040 I mmtk-art64: mmtk::plan::sticky::immix::global: Nursery GC
08-01 12:22:49.838  1025  1040 I mmtk-art64: mmtk::scheduler::scheduler: End of GC (7385/7991 pages, took 24 ms)
[...]
```

## Known Limitations

The `StickyImmix` plan is only supported for x86_64 as the write barriers have not been implemented for any other platform.

Currently we disable SELinux for the Cuttlefish VM at boot to avoid issues when trying to read `/proc` files.

The port has not been performance tuned at all.
For example, currently the write barrier is a full call into MMTk even for the fast-path.

There are also major features missing from MMTk currently such as the ability to return free pages back to the operating system, which is obviously a key requirement for mobile devices.
