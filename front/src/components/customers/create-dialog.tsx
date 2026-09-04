"use client";

import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ApiError, createCustomer } from "@/lib/api";
import {
  CustomerFormFields,
  EMPTY_FORM,
  toFields,
  type FormValues,
} from "./customer-form";
import { Notice } from "./notice";

const ID_LENGTH = 5;
const ID_PATTERN = /^[A-Z0-9]{5}$/;

export function CreateCustomerDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: () => void;
}) {
  const [customerId, setCustomerId] = useState("");
  const [values, setValues] = useState<FormValues>(EMPTY_FORM);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const idValid = ID_PATTERN.test(customerId);
  const nameValid = values.companyName.trim().length > 0;

  // El 409 se enseña pegado al campo que lo provoca, no en el banner de arriba: el
  // usuario tiene que cambiar ESE dato, y el mensaje debe estar donde va a mirar.
  const duplicateId = error instanceof ApiError && error.code === "duplicate_id";

  function reset() {
    setCustomerId("");
    setValues(EMPTY_FORM);
    setError(null);
    setSubmitting(false);
  }

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    if (submitting || !idValid || !nameValid) return;

    setSubmitting(true);
    setError(null);

    try {
      await createCustomer({ customerId, ...toFields(values) });
      reset();
      onOpenChange(false);
      onCreated();
    } catch (caught) {
      setError(caught instanceof Error ? caught : new Error(String(caught)));
    } finally {
      // 🇪🇸 NOTA: el `finally` es importante. Si el `setSubmitting(false)` viviera
      // solo en el camino feliz, un error dejaría el botón deshabilitado para
      // siempre y el diálogo habría que cerrarlo y volver a abrirlo para reintentar.
      setSubmitting(false);
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) reset();
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Nuevo cliente</DialogTitle>
          <DialogDescription>
            El identificador no se puede cambiar después.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex min-h-0 flex-col gap-4">
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            <div>
              <div className="mb-1.5 flex items-baseline justify-between">
                <Label htmlFor="customerId">Customer ID *</Label>
                {/* Contador: la regla de los 5 caracteres es del backend y no es
                    evidente. Verla mientras se escribe evita el 400. */}
                <span
                  className={`tabular text-xs ${
                    customerId.length === ID_LENGTH
                      ? "text-muted-foreground"
                      : "text-muted-foreground/60"
                  }`}
                >
                  {customerId.length}/{ID_LENGTH}
                </span>
              </div>
              <Input
                id="customerId"
                value={customerId}
                maxLength={ID_LENGTH}
                autoComplete="off"
                disabled={submitting}
                aria-invalid={duplicateId || undefined}
                // 🇪🇸 NOTA (mayúsculas MIENTRAS se escribe, no al enviar): el
                // backend normaliza igualmente, pero si la caja enseña "alfki" y lo
                // guardado es "ALFKI", el usuario ve que la aplicación le cambia lo
                // que escribió por detrás. Transformar en el momento hace que lo que
                // se ve sea siempre lo que se va a guardar.
                onChange={(event) =>
                  setCustomerId(
                    event.target.value.toUpperCase().replace(/[^A-Z0-9]/g, ""),
                  )
                }
                className="tabular w-40"
              />
              {duplicateId ? (
                <p className="mt-1.5 text-xs text-destructive">
                  {error.message}
                </p>
              ) : (
                <p className="mt-1.5 text-xs text-muted-foreground/80">
                  5 caracteres alfanuméricos, por ejemplo ALFKI.
                </p>
              )}
            </div>

            <CustomerFormFields
              values={values}
              disabled={submitting}
              onChange={(name, value) =>
                setValues((previous) => ({ ...previous, [name]: value }))
              }
            />
          </div>

          {error && !duplicateId ? (
            <Notice variant="error" title="No se pudo crear el cliente">
              {error.message}
            </Notice>
          ) : null}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={submitting}
            >
              Cancelar
            </Button>
            {/* 🇪🇸 NOTA (por qué se deshabilita durante el envío): sin esto, un doble
                clic manda DOS POST con el mismo id. El primero crea el cliente y el
                segundo recibe un 409 "ya existe" — el usuario ve un error de
                duplicado sobre un cliente que acaba de crear él mismo, hace dos
                décimas de segundo. Es de los mensajes más desconcertantes que puede
                dar un formulario, y se evita con un atributo. */}
            <Button type="submit" disabled={submitting || !idValid || !nameValid}>
              {submitting ? "Creando…" : "Crear cliente"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
