// D&D の操作 (copy/move) を決定する
// - Ctrl 押下 → 強制 copy
// - 内部 D&D (アプリ内) → move
// - 外部 D&D → src と dst のボリューム比較 (同一=move / 別=copy)
//
// 注意: 複数ファイルがドライブ混在の場合は paths[0] を代表として判定する。
//       将来「ドライブごとに分割実行」が必要になったら policy 側で配列を返す形に拡張する。
import { volumeOf } from "../path-util";

export type DropOp = "copy" | "move";

export interface DecideOpInput {
  ctrlKey: boolean;
  isInternal: boolean;
  srcPaths: string[];
  dstPath: string;
}

export function decideOp(input: DecideOpInput): DropOp {
  if (input.ctrlKey) return "copy";
  if (input.isInternal) return "move";
  if (!input.srcPaths.length) return "copy";
  const srcVol = volumeOf(input.srcPaths[0]);
  const dstVol = volumeOf(input.dstPath);
  return srcVol && dstVol && srcVol === dstVol ? "move" : "copy";
}
