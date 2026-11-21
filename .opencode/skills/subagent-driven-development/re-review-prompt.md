# Scoped fix reviewer brief

Derived from obra/superpowers; see LICENSE.upstream.

Provide original requirements, previous findings, fix baseline/candidate, and fix
diff. Remain read-only and do not delegate. Verify each finding against the fix and
check new breakage introduced by it. Use focused independent checks when necessary.
Return ADDRESSED/NOT ADDRESSED with evidence per finding and PASS/FAIL/NEEDS_REVIEW
for the fix. List unrelated observations separately; do not expand the fix loop.
