import { Suspense } from "react";

import { CustomersPage } from "@/components/customers/customers-page";

/**
 * 🇪🇸 NOTA (por qué este componente de servidor no hace nada más que un `Suspense`):
 *
 * Todo el panel es cliente —lee `useSearchParams`, mantiene estado, habla con la API
 * desde el navegador—, pero `useSearchParams()` obliga a que haya un límite de
 * Suspense por encima. La razón: durante el prerenderizado del build, Next no conoce
 * la query string, así que necesita un punto donde parar y dejar ese trozo para el
 * cliente. Sin el `Suspense`, `next build` falla con "useSearchParams should be
 * wrapped in a suspense boundary".
 *
 * Este archivo es, por tanto, el único componente de servidor del proyecto, y no
 * hace ni una petición: solo marca la frontera.
 */
export default function Page() {
  return (
    <Suspense>
      <CustomersPage />
    </Suspense>
  );
}
