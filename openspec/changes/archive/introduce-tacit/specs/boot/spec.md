## Purpose

Covers freestanding load under QEMU `aarch64` virt: from machine reset to a documented ready state, with no host OS in the guest. The development host is an Apple Silicon Mac.

## ADDED Requirements

### Requirement: QEMU aarch64 virt is the first-class target
The system MUST boot as a freestanding guest under QEMU `aarch64` virt. The project MUST document a single build command and a single run command. On an Apple Silicon Mac the documented run command MUST use HVF acceleration when the hypervisor is available.

#### Scenario: Documented run command
- **GIVEN** a clean checkout and the documented toolchain on an Apple Silicon Mac
- **WHEN** an operator runs the documented build command, then the documented QEMU command
- **THEN** QEMU starts the built AArch64 image without a preinstalled guest OS
- **AND** the operator does not have to assemble loader flags by hand
- **AND** the run command requests HVF when that accelerator is present

#### Scenario: Wrong architecture is rejected at build
- **GIVEN** the first-milestone target is QEMU `aarch64` virt
- **WHEN** a build is requested for an unsupported boot architecture
- **THEN** the build fails with an explicit unsupported-target error
- **AND** it does not emit a silently unbootable image

#### Scenario: x86_64 is not the first-milestone target
- **GIVEN** the first-milestone image
- **WHEN** its architecture is inspected
- **THEN** it is AArch64
- **AND** a required x86_64 boot path is out of contract for this change

### Requirement: Small early boot
Early boot MUST do only architecture bring-up: exception levels and page tables if required, a kernel stack, a framebuffer or text console, a physical memory map, and a machine description. It MUST then transfer control to the Uiua/UIR runtime and an initial Realm.

#### Scenario: Ready banner from the initial Realm
- **GIVEN** QEMU has just started the AArch64 image
- **WHEN** boot completes
- **THEN** the display shows a fixed ready banner that includes the name Tacit
- **AND** an initial Realm is running
- **AND** the system can start the bundled tiny program or accept keyboard events, as documented

#### Scenario: Boot failure is visible
- **GIVEN** a boot fault before the ready state (missing memory map, display init failure, or allocator init failure)
- **WHEN** the fault is detected
- **THEN** the system stops in a halt state
- **AND** it emits a distinct diagnostic on the display or on the QEMU debug console
- **AND** it does not pretend to have reached the ready state

### Requirement: No guest host OS
The boot path MUST load the project's own runtime directly. It MUST NOT kexec, chain-load, or otherwise start a general-purpose guest operating system. It MUST NOT boot macOS, Linux, or a hosted Uiua interpreter as the guest kernel.

#### Scenario: Image contents
- **GIVEN** a built boot image
- **WHEN** the image is inspected
- **THEN** it contains the project loader, microkernel, UIR payloads, and a machine description
- **AND** it does not contain a Linux, BSD, macOS, or other general-purpose kernel

### Requirement: Native metal boot is out of this change
The first milestone MUST NOT require iBoot, a custom Apple Device Tree, or Asahi-style bring-up of AIC, DART, or the display coprocessor. A later change MAY add a native Apple Silicon boot path without changing the Realm, UIR, or placement model.

#### Scenario: Virt machine is sufficient
- **GIVEN** the first-milestone run command
- **WHEN** the image boots
- **THEN** it boots on QEMU `aarch64` virt
- **AND** it does not require Apple firmware or a metal device tree
