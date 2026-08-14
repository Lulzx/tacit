# Fusion bench: C = (A + B) × D over a documented large shape (2048x2048 f32).
# The host compiler emits this both fused (one AddMul kernel) and unfused
# (separate Add and Multiply), and the bench reports payload bytes for both.
A ← &fill [2048 2048] 1.0
B ← &fill [2048 2048] 2.0
D ← &fill [2048 2048] 3.0
A B + D ×
