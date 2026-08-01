import { useState } from 'react';
const KEY = 'km:onboarding-banner-dismissed';
export function OnboardingBanner() {
  const [hidden, setHidden] = useState(() => sessionStorage.getItem(KEY) === '1');
  if (hidden) return null;
  return (
    <div className="banner" role="note">
      {' '}
      <span>
        {' '}
        <strong>Get started:</strong> 1) add your API token in Settings, 2) drop a share image
        anywhere (or paste) to create a Character. On mobile, tap the import area to pick a file. 3) add a Target, 4) push.{' '}
      </span>{' '}
      <button
        onClick={() => {
          sessionStorage.setItem(KEY, '1');
          setHidden(true);
        }}
      >
        {' '}
        Dismiss{' '}
      </button>{' '}
    </div>
  );
}
