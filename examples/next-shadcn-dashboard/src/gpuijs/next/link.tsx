// Next.js stand-in: an anchor whose click navigates in place.
import type { AnchorHTMLAttributes, MouseEvent } from 'react';
import { router } from '../router';

type Props = AnchorHTMLAttributes<HTMLAnchorElement> & { href: string; replace?: boolean; prefetch?: boolean; scroll?: boolean };

export default function Link({ replace, prefetch: _, scroll: __, ...props }: Props) {
  const onClick = (e: MouseEvent<HTMLAnchorElement>) => {
    props.onClick?.(e);
    if (e.defaultPrevented || props.target === '_blank' || /^[a-z]+:/i.test(props.href)) return;
    e.preventDefault();
    (replace ? router.replace : router.push)(props.href);
  };
  return <a {...props} onClick={onClick} />;
}
