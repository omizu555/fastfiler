#!/usr/bin/env bash
# Stop フック: Rust ソースに未コミット変更があるのに「作業後セルフレビュー」が
# 未実施のまま停止しようとしたら、1 回だけブロックしてレビューを促す。
#
# 解除 (スタンプ): セルフレビュー実施後に  date > .claude/.selfreview-stamp
# 仕組み: 変更された .rs のうちスタンプより新しいものがあればレビュー未実施とみなす。
# stop_hook_active=true (ブロック後の継続中) では再ブロックしない (無限ループ防止)。
set -u

input=$(cat 2>/dev/null || true)
case "$input" in
  *'"stop_hook_active":true'*) exit 0 ;;
esac

cd "${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}" || exit 0

# 未コミットの .rs 変更 (staged/unstaged/未追跡)。リネームは新パス側を採用。
changed=$(git status --porcelain -- '*.rs' 2>/dev/null | sed 's/^...//; s/.* -> //')
[ -z "$changed" ] && exit 0

stamp=".claude/.selfreview-stamp"
if [ -f "$stamp" ]; then
  need=0
  while IFS= read -r f; do
    [ -n "$f" ] && [ -f "$f" ] && [ "$f" -nt "$stamp" ] && { need=1; break; }
  done <<EOF
$changed
EOF
  [ "$need" -eq 0 ] && exit 0
fi

reason="Rust ソースに未コミット変更がありますが、作業後セルフレビューが未実施です。.claude/skills/iced-rewrite/SKILL.md の『作業後セルフレビュー』を実施してください: (1) cargo fmt/clippy/test (2) 改善提案 (高速化/簡潔化/設計/テストの 4 観点、file:line 付き) を最終報告に含める (3) 一般化できる学びを .claude/skills/iced-rewrite/LESSONS.md へ追記 (4) 完了後に date > .claude/.selfreview-stamp を実行してから停止。作業が途中の場合は、その旨と現状を報告した上でスタンプを実行して停止してよい (レビューは完了時にまとめて実施)。"
printf '{"decision":"block","reason":"%s"}\n' "$reason"
exit 0
