import type { ReactNode } from "react";

interface Props {
  priority: number;
  maxStars?: number;
}

export function StarRating({ priority, maxStars = 5 }: Props) {
  const stars: ReactNode[] = [];
  for (let i = 1; i <= maxStars; i++) {
    if (priority >= i) {
      // Full star
      stars.push(<span key={i} className="star star-full">&#9733;</span>);
    } else if (priority >= i - 0.5) {
      // Half star
      stars.push(
        <span key={i} className="star star-half">
          <span className="star-half-filled">&#9733;</span>
          <span className="star-half-empty">&#9733;</span>
        </span>,
      );
    } else {
      // Empty star
      stars.push(<span key={i} className="star star-empty">&#9733;</span>);
    }
  }
  return <span className="star-rating">{stars}</span>;
}
