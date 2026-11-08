// Next.js stand-in: an unoptimized <img>.
import type { ImgHTMLAttributes } from 'react';

type Props = ImgHTMLAttributes<HTMLImageElement> & { fill?: boolean; priority?: boolean; quality?: number };

export default function Image({ fill, priority: _, quality: __, ...props }: Props) {
  return <img {...props} style={fill ? { position: 'absolute', inset: 0, width: '100%', height: '100%', ...props.style } : props.style} />;
}
