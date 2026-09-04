"use client";

import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { getCustomer, updateCustomer } from "@/lib/api";
import {
  CustomerFormFields,
  EMPTY_FORM,
  formFromCustomer,
  toFields,
  type FormValues,
} from "./customer-form";
import { Notice } from "./notice";

export function EditCustomerDialog({
  customerId,
  onOpenChange,
  onUpdated,
}: {
  /** `null` = cerrado. El id abierto es a la vez el estado del diálogo. */
  customerId: string | null;
  onOpenChange: (open: boolean) => void;
  onUpdated: () => void;
}) {
  const [values, setValues] = useState<FormValues>(EMPTY_FORM);
  const [loading, setLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  /** El GET de precarga falló: no hay datos fiables sobre los que guardar. */
  const [loadFailed, setLoadFailed] = useState(false);

  /**
   * 🇪🇸 NOTA (por qué se hace un GET al abrir en vez de reutilizar la fila de la
   * tabla): la fila puede llevar minutos en pantalla y otro operador puede haberla
   * cambiado. Editar sobre datos viejos y guardar los 11 campos DESHARÍA su cambio
   * sin que nadie se entere — y como el PUT es reemplazo total, no se pierde solo el
   * campo que yo toco: se pierden todos los suyos.
   *
   * Además, la tabla no trae los 11 campos (oculta address, phone, fax…), así que
   * sin este GET habría que enviar nulos en lo que no se muestra. El GET es una
   * petición barata que convierte "editar" en una operación honesta.
   */
  useEffect(() => {
    if (!customerId) return;

    const controller = new AbortController();

    // ⚠️ Se vacía el formulario ANTES de pedir los datos nuevos. Sin esto, los
    // valores que quedan en el estado son los del cliente ANTERIOR, y como el PUT es
    // reemplazo total, guardar ahí escribiría el registro de A encima del id de B —
    // los diez campos, de golpe y sin un solo error. El skeleton tapa el formulario
    // mientras `loading`, pero eso es una tapadera visual: si el GET falla, el
    // skeleton desaparece y los datos viejos quedan a la vista y editables.
    setValues(EMPTY_FORM);
    setLoadFailed(false);
    setLoading(true);
    setError(null);

    getCustomer(customerId, controller.signal)
      .then((customer) => {
        setValues(formFromCustomer(customer));
        setLoading(false);
      })
      .catch((caught: unknown) => {
        if (caught instanceof DOMException && caught.name === "AbortError")
          return;
        // Guardar queda cerrado hasta que haya una precarga con éxito: sin saber qué
        // había en el registro no se puede reemplazarlo entero de forma honesta.
        setLoadFailed(true);
        setError(caught instanceof Error ? caught : new Error(String(caught)));
        setLoading(false);
      });

    return () => controller.abort();
  }, [customerId]);

  const nameValid = values.companyName.trim().length > 0;

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    if (!customerId || submitting || loading || loadFailed || !nameValid) return;

    setSubmitting(true);
    setError(null);

    try {
      // ⚠️ Se envían LOS DIEZ campos, no solo los que el usuario tocó. El PUT de
      // esta API es reemplazo total: lo que no viaje en el cuerpo se guarda como
      // NULL. Un "envía solo lo modificado" iría vaciando el registro en cada
      // guardado, sin un solo error y sin que nadie lo note hasta que falte un
      // teléfono.
      await updateCustomer(customerId, toFields(values));
      onOpenChange(false);
      onUpdated();
    } catch (caught) {
      setError(caught instanceof Error ? caught : new Error(String(caught)));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={customerId !== null} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            Editar cliente{" "}
            <span className="tabular font-mono text-sm text-muted-foreground">
              {customerId}
            </span>
          </DialogTitle>
          <DialogDescription>
            El identificador no se puede modificar.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex min-h-0 flex-col gap-4">
          <div className="min-h-0 flex-1 overflow-y-auto pr-1">
            <CustomerFormFields
              values={values}
              loading={loading}
              disabled={submitting}
              onChange={(name, value) =>
                setValues((previous) => ({ ...previous, [name]: value }))
              }
            />
          </div>

          {error ? (
            <Notice
              variant="error"
              title={
                loadFailed ? "No se pudo cargar el cliente" : "No se pudo guardar"
              }
            >
              {error.message}
            </Notice>
          ) : null}

          <DialogFooter className="sm:items-center sm:justify-between">
            {/* 🇪🇸 NOTA: el aviso va junto al botón que dispara la acción, no en la
                cabecera. Ahí es donde el usuario mira antes de pulsar, y lo que dice
                —que se escriben todos los campos— es una consecuencia del PUT que
                nadie adivinaría desde la interfaz. */}
            <p className="text-xs text-muted-foreground/80">
              Al guardar se actualizan todos los campos de este cliente.
            </p>
            <div className="flex flex-col-reverse gap-2 sm:flex-row">
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
                disabled={submitting}
              >
                Cancelar
              </Button>
              <Button
                type="submit"
                disabled={submitting || loading || loadFailed || !nameValid}
              >
                {submitting ? "Guardando…" : "Guardar cambios"}
              </Button>
            </div>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
