## 1. Preconditions (unikernel + named UIR)

- [ ] 1.1 Confirm a build that loads `C = (A + B) × D` with named Add and Multiply nodes still visible after load
- [ ] 1.2 Confirm the image has no POSIX file path, no listen/accept, and only the devices that milestone needs
- [ ] 1.3 Add in-image counters: payload bytes moved, payload bytes copied, kernel entries

## 2. Fusion (first number)

- [ ] 2.1 Implement a host fusion pass for single-consumer pure elementwise chains
- [ ] 2.2 Fuse Add-then-Multiply so T = A+B is not allocated
- [ ] 2.3 Keep a documented unfused mode for comparison
- [ ] 2.4 Refuse to fuse across display/keyboard effects and across undocumented fan-out
- [ ] 2.5 Ship `bench-fusion`: same C, fused bytes < unfused bytes, both printed

## 3. Zero-copy send (second number)

- [ ] 3.1 Make immutable same-placement send a region-capability share or move
- [ ] 3.2 Keep unique-region in-place mutation
- [ ] 3.3 Keep an explicit copy transform as the bench control
- [ ] 3.4 Ship `bench-send`: cap path copies fewer payload bytes than memcpy on the documented large array

## 4. Datapath and placement

- [ ] 4.1 Run the fused kernel as one entry (or documented tiles), not one trap per element
- [ ] 4.2 Deliver keyboard as one array per line, not one syscall per key
- [ ] 4.3 Record `place = host` on fused nodes; do not add a CUDA-style API
- [ ] 4.4 Document the next change (device placement) as retargeting the same UIR, not a new language
