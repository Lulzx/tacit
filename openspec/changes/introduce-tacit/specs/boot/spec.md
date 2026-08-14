## Purpose

Covers freestanding load under QEMU: from machine reset to a documented ready state, with no host OS in the guest.

## ADDED Requirements

### Requirement: QEMU is the first-class target
The system MUST boot as a freestanding guest under QEMU on x86_64. The project MUST document a single build command and a single run command.

#### Scenario: Documented run command
- **GIVEN** a clean checkout and the documented toolchain
- **WHEN** an operator runs the documented build command, then the documented QEMU command
- **THEN** QEMU starts the built image without a preinstalled guest OS
- **AND** the operator does not have to assemble loader flags by hand

#### Scenario: Wrong architecture is rejected at build
- **GIVEN** the first-milestone target is x86_64 QEMU
- **WHEN** a build is requested for an unsupported boot architecture
- **THEN** the build fails with an explicit unsupported-target error
- **AND** it does not emit a silently unbootable image

### Requirement: Small early boot
Early boot MUST do only architecture bring-up: page tables if required, a kernel stack, a framebuffer or text console, and a machine description. It MUST then transfer control to the Uiua/UIR runtime and an initial Realm.

#### Scenario: Ready banner from the initial Realm
- **GIVEN** QEMU has just started the image
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
The boot path MUST load the project's own runtime directly. It MUST NOT kexec, chain-load, or otherwise start a general-purpose guest operating system.

#### Scenario: Image contents
- **GIVEN** a built boot image
- **WHEN** the image is inspected
- **THEN** it contains the project loader, microkernel, and UIR payloads
- **AND** it does not contain a Linux, BSD, or other general-purpose kernel
