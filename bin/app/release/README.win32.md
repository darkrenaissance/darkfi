# Windows Build Guide (MSVC)

Skip the first step if you're already using Windows.

## Prepare the VM

You will need qemu and the remote-viewer tool.

Provision a disk:

```
qemu-img create -f raw winblows-raw.disk 100G
```

Download the Windows ISO from their website. Use this script to launch QEMU.

```
#!/bin/bash

ISO=Win10_22H2_EnglishInternational_x64v1.iso

args=(
    --cdrom $ISO --boot order=d

    -drive file=winblows-disk.raw,format=raw

    -m 30G -accel kvm -cpu qemu64

    # We forward 22 to 10022 for SSH
    #-net nic -net user,hostname=windowsvm
    -net nic -net user,hostname=windowsvm,hostfwd=tcp::10146-:22

    # This fixes the fucked up mouse
    #-device qemu-xhci -device usb-mouse -device usb-tablet
    -machine vmport=off

    # Auto-resize display
    # -vga qxl
    # We use virtio since it allows us the full res size at least
    -vga virtio -spice port=30001,disable-ticketing=on
    -device virtio-serial -chardev spicevmc,id=vdagent,debug=0,name=vdagent
    -device virtserialport,chardev=vdagent,name=com.redhat.spice.0
)

qemu-system-x86_64 "${args[@]}"
```

There will be no output. Use remote-viewer to attach the display:

```
remote-viewer spice://localhost:30001
```

Now install Windows. Then power off Windows. Download the [virtio-win ISO].
Modify the `ISO=...` line of the script above and relaunch the VM.

Navigate to the CD drive in the file explorer and install the virtio x64 driver.

In your browser go to "spice windows guest" and scroll down the webpage.
Download and install "Windows SPICE Guest Tools".

Relaunch the Windows VM. Adjust your display resolution and fullscreen the VM.

### (Optional) Enable SSH

This will enable you to work on Windows from within your host.

Open Settings -> Apps -> Optional features -> + Add a feature. Search
for "OpenSSH Server" and install it.

Open Services -> OpenSSH SSH Server. Make "Startup type" Automatic.

You can now ssh into your windows and use cmd.exe. In the script above,
we forward port 22 to 10146. You can put this in `~/.ssh/config`.

```
Host winblows
    Hostname localhost
    User a
    Port 10146
```

You can also make an `/etc/fstab` entry with:

```
sshfs#winblows: 		/mnt/winblows  		fuse  	noauto,defaults  	0  	0
```

Then `mount /mnt/winblows && cd /mnt/winblows/.ssh/` and copy your SSH pubkey
to `authorized_keys`.
Open "This PC", View -> Hidden files, open `C:\ProgramData\ssh\sshd_config`
to disable password login and just use pubkey auth.
Also disable the administrator auth keys setting in there too (bottom 2 lines).
Then restart SSH.

## Setting Up the Dev Environment

Install rustup, which will also install Visual Studio. Next, next, finish.
After visual studio, it will then proceed with the rustup install.
Select 2 and enter nightly.

```
1) Proceed with standard installation (default - just press enter)
2) Customize installation
3) Cancel installation
>2

Default host triple? [x86_64-pc-windows-msvc]
(leave this unchanged)

Default toolchain? (stable/beta/nightly/none)
nightly

Profile (which tools and data to install)? (minimal/default/complete) [default]
(leave this unchanged)
```

Then proceed with the installation (option 1).

## Building the DarkFi App

Go to the [codeberg repo] and select "⋯", then Download ZIP. Unzip the folder
in an accessible place.

Open cmd and navigate to the folder. Now run `cargo build`.

```
C:\Users\a> cd ../../darkfi/bin/app/
C:\darkfi\bin\app> cargo build
```

## (Optional) Mesa GL

This is buggy af software renderer.

* Setup OpenGL using [this guide](https://thomas.inf3.ch/2019-06-12-opengl-kvm-mesa3d/index.html).
    * Download [mesa3d-xxx-release-msvc.7z](https://github.com/pal1000/mesa-dist-win/releases)
      and install the default options.

[virtio-win ISO]: https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/latest-virtio/virtio-win.iso

