import type { ReactNode } from "react";

interface Props {
  priority: number;
  maxStars?: number;
}

import { Star } from "lucide-react";

export function StarRating({ priority, maxStars = 5 }: Props) {
  const stars: ReactNode[] = [];
  for (let i = 1; i <= maxStars; i++) {
    if (priority >= i) {
      // Full star
      stars.push(
        <Star key={i} className="w-3.5 h-3.5 fill-[#C9A84C] text-[#C9A84C]" />
      );
    } else if (priority >= i - 0.5) {
      // Half star
      stars.push(
        <div key={i} className="relative w-3.5 h-3.5">
          <Star className="absolute inset-0 w-3.5 h-3.5 text-[#2A2A35]" />
          <div className="absolute inset-0 overflow-hidden w-[50%]">
            <Star className="w-3.5 h-3.5 fill-[#C9A84C] text-[#C9A84C]" />
          </div>
        </div>
      );
    } else {
      // Empty star
      stars.push(
        <Star key={i} className="w-3.5 h-3.5 text-[#2A2A35]" />
      );
    }
  }
  return <div className="flex items-center gap-0.5">{stars}</div>;
}
