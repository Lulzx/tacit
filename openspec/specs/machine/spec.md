## Purpose

Defines first-milestone hardware facing on QEMU `aarch64` virt, plus a machine description of the Apple M4 Pro the image is being built toward: engines, page size, cache line, unified memory.

## ADDED Requirements

### Requirement: Visible text output
The system MUST present a text console the operator can read inside QEMU, using a framebuffer or a text-mode display. Characters written through a display capability MUST appear on that console.

#### Scenario: Program text is visible
- **GIVEN** the system has reached the ready state
- **WHEN** the bundled tiny Uiua program writes a result
- **THEN** that result appears as readable text on the QEMU display
- **AND** the text remains visible until later output scrolls or overwrites it

#### Scenario: Output longer than one line
- **GIVEN** a program that writes more lines than fit on the screen
- **WHEN** those lines are emitted
- **THEN** earlier lines scroll or are replaced in a documented way
- **AND** the newest line remains fully readable

### Requirement: Keyboard input
The system MUST accept key presses from the QEMU keyboard (virtio-input or the documented virt default) and publish them as events. A granted reader MUST receive characters or key events. Unmapped keys MUST NOT crash the machine.

#### Scenario: Printable key
- **GIVEN** the ready state and a transform that reads input
- **WHEN** the operator presses a printable key
- **THEN** the corresponding character is delivered to that transform
- **AND** the character is echoed unless silent input was requested

#### Scenario: Enter completes a line
- **GIVEN** a line-oriented reader
- **WHEN** the operator types characters and presses Enter
- **THEN** the reader receives one character array for that line
- **AND** a new display line begins

#### Scenario: Unknown or non-text key
- **GIVEN** a modifier-only or unmapped key
- **WHEN** that key is pressed
- **THEN** the system remains in the ready state
- **AND** it does not halt or corrupt kernel memory

### Requirement: Physical memory ownership
The machine MUST own a physical memory map and MUST refuse allocations that overlap the kernel image, stack, or display buffer. Allocation failure MUST be reported. Guest pages MUST be 16 KiB.

#### Scenario: Successful allocation
- **GIVEN** free memory large enough for a requested region
- **WHEN** the region is allocated
- **THEN** the caller receives a usable region of at least the requested size
- **AND** the region does not overlap the kernel image, stack, or display buffer
- **AND** the region is aligned to 16 KiB

#### Scenario: Allocation failure
- **GIVEN** free memory smaller than the request
- **WHEN** the region is requested
- **THEN** the request fails with an allocation error
- **AND** previously allocated live regions remain intact

### Requirement: Interrupts are captured, not programmed
Hardware interrupts MUST be acknowledged in the machine layer and turned into timestamped event records. Programs MUST NOT register C-style interrupt service routines.

#### Scenario: Key IRQ becomes a record
- **GIVEN** the ready state
- **WHEN** a key IRQ arrives
- **THEN** the machine appends one event record
- **AND** no program-supplied ISR runs in interrupt context

### Requirement: Machine description names engines
The boot image MUST carry a machine description that names the reference Apple M4 Pro inventory even when the virt guest only wires the boot CPU. The description MUST include at least: P-cores, E-cores, NEON, SME, GPU, ANE, media, display, unified-memory home, 16 KiB pages, and 128-byte cache lines. The first milestone MUST mark only the boot CPU as online.

#### Scenario: Description is readable after ready
- **GIVEN** the ready state
- **WHEN** the machine description is inspected
- **THEN** it lists P-cores, E-cores, NEON, SME, GPU, ANE, media, and display as engines
- **AND** it records `home = uma`, 16 KiB pages, and 128-byte lines
- **AND** only the boot CPU is marked online

#### Scenario: Offline engines do not crash the tiny program
- **GIVEN** SME, GPU, and ANE marked offline
- **WHEN** the bundled tiny program runs
- **THEN** it completes on the boot CPU
- **AND** the system does not attempt to dispatch those engines

### Requirement: Virt is not the metal
The first-milestone guest MUST NOT require Apple-specific interrupt controllers, DART, the display coprocessor, or the Secure Enclave. Those devices MAY appear in the machine description as offline. A later change MAY bring them online on a native boot path.

#### Scenario: Missing Apple device is not a boot fault
- **GIVEN** QEMU `aarch64` virt
- **WHEN** AIC, DART, or the display coprocessor is absent
- **THEN** boot still reaches the ready banner
- **AND** the diagnostic path is not taken for those absences
