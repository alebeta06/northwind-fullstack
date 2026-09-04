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
import { ApiError, deleteCustomer, type Customer } from "@/lib/api";
import { Notice } from "./notice";

export function DeleteCustomerDialog({
  customer,
  onOpenChange,
  onDeleted,
}: {
  /** `null` = cerrado. */
  customer: Customer | null;
  onOpenChange: (open: boolean) => void;
  onDeleted: () => void;
}) {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  // Al abrir con otro cliente hay que limpiar el aviso del anterior: si no, el
  // diálogo se abriría ya enseñando el "tiene pedidos" de la fila de antes.
  useEffect(() => {
    setError(null);
    setSubmitting(false);
  }, [customer?.customerId]);

  /**
   * ═══════════════════════════════════════════════════════════════════
   * 🇪🇸 NOTA — EL 409 NO ES UN ERROR, Y AQUÍ ES EL CAMINO NORMAL
   * ═══════════════════════════════════════════════════════════════════
   *
   * Los 93 clientes originales de Northwind tienen pedidos, así que borrar
   * cualquiera de ellos devuelve 409 `has_orders`. No es un caso raro que haya que
   * contemplar por si acaso: es lo que va a pasar casi siempre.
   *
   * Tratarlo como un fallo tendría tres consecuencias, todas malas:
   *
   *   1. El diálogo se cerraría y el mensaje se perdería en un toast, dejando al
   *      usuario mirando una tabla que no ha cambiado sin saber por qué.
   *   2. En rojo, leería "he roto algo" — cuando lo que ha pasado es que el sistema
   *      ha protegido 163 pedidos de quedarse huérfanos.
   *   3. Ofrecer "Reintentar" invitaría a repetir una operación cuyo resultado será
   *      idéntico las mil veces siguientes.
   *
   * Por eso: el diálogo NO se cierra, el aviso es informativo, y el botón primario
   * pasa a ser "Entendido". El único camino que queda abierto es el correcto —
   * cerrar y, si de verdad hay que borrarlo, ocuparse antes de sus pedidos.
   */
  const businessRule = error instanceof ApiError && error.code === "has_orders";

  async function handleDelete() {
    if (!customer || submitting) return;

    setSubmitting(true);
    setError(null);

    try {
      await deleteCustomer(customer.customerId);
      onOpenChange(false);
      onDeleted();
    } catch (caught) {
      setError(caught instanceof Error ? caught : new Error(String(caught)));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={customer !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>
            {businessRule ? "Este cliente no se puede borrar" : "Borrar cliente"}
          </DialogTitle>
          {!businessRule ? (
            <DialogDescription>
              Se va a borrar{" "}
              {/* El nombre de la empresa, no solo el id: "¿borrar ALFKI?" obliga a
                  recordar qué es ALFKI, y ahí es donde se borra el cliente
                  equivocado. */}
              <span className="font-medium text-foreground">
                {customer?.companyName}
              </span>{" "}
              <span className="tabular text-muted-foreground">
                ({customer?.customerId})
              </span>
              . Esta acción no se puede deshacer.
            </DialogDescription>
          ) : null}
        </DialogHeader>

        {error ? (
          businessRule ? (
            <Notice variant="info">{error.message}</Notice>
          ) : (
            <Notice variant="error" title="No se pudo borrar">
              {error.message}
            </Notice>
          )
        ) : null}

        <DialogFooter>
          {businessRule ? (
            <Button type="button" onClick={() => onOpenChange(false)}>
              Entendido
            </Button>
          ) : (
            <>
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
                disabled={submitting}
              >
                Cancelar
              </Button>
              <Button
                type="button"
                variant="destructive"
                onClick={handleDelete}
                disabled={submitting}
              >
                {submitting
                  ? "Borrando…"
                  : error
                    ? "Reintentar"
                    : "Borrar cliente"}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
