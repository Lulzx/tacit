# SME matrix bench: &matmul is placed on engine=sme.  The first slice enters
# streaming mode (proving the SME engine executes on this machine), then
# computes the product; engine entries report whether SME or the p-core
# fallback ran.  SME is online under TCG `-cpu max`; HVF may keep it offline.
A ← ↯ [64 64] 1.5
B ← ↯ [64 64] 2.0
&zero
C ← &matmul A B
&display &fmt "C00 (64 * 3 = 192): " ⊡ 0 ⊡ 0 C
&display &stats
