# Capability authorization policy, in Uiua.
# Given the capabilities table (&caps: [kind, region, rights, held]) and the
# requested cap kind K (&request, set by the kernel), return the count of
# held caps of that kind.  The kernel treats count > 0 as authorized.  The
# token *verification* (PAC) stays in Rust; this program is the policy.
C ← &caps
K ← &request
M ← = K ⊡ 0 C        # kind==K mask [n]
H ← ⊡ 3 C            # held column [n]
A ← × M H            # kind==K AND held (elementwise) [n]
⧻ ▽ A C              # count of authorized rows
