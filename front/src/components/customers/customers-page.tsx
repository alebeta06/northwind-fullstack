"use client";

import { useEffect, useState } from "react";
import { ChevronLeft, ChevronRight, Plus, Search } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { NetworkError, type Customer } from "@/lib/api";
import { useCustomers } from "@/lib/use-customers";
import { PAGE_SIZES, useListParams } from "@/lib/use-list-params";
import { CreateCustomerDialog } from "./create-dialog";
import { CustomersTable } from "./customers-table";
import { DeleteCustomerDialog } from "./delete-dialog";
import { EditCustomerDialog } from "./edit-dialog";
import { Notice } from "./notice";

const SEARCH_DEBOUNCE_MS = 300;

export function CustomersPage() {
  const { params, setParams, toggleSort } = useListParams();
  const { page, loading, error, reload } = useCustomers(params);

  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<Customer | null>(null);

  // ── Buscador con debounce ───────────────────────────────────────────
  //
  // 🇪🇸 NOTA: el input tiene su propio estado porque debe responder a cada tecla sin
  // esperar a nadie. Lo que va con retraso es la CONSULTA. Sin debounce, escribir
  // "restaurant" lanzaría diez peticiones y diez cambios de URL; con 300 ms, una.
  //
  // El número no es arbitrario: por debajo de ~200 ms se sigue disparando entre
  // pulsaciones de alguien que escribe rápido, y por encima de ~400 ms la interfaz
  // empieza a sentirse dormida.
  const [term, setTerm] = useState(params.companyName);

  useEffect(() => {
    if (term === params.companyName) return;

    const timer = setTimeout(() => {
      // Cualquier cambio de filtro vuelve a la página 1: quedarse en la 5 de un
      // resultado que ahora tiene 2 páginas enseñaría una tabla vacía.
      setParams({ companyName: term, page: 1 });
    }, SEARCH_DEBOUNCE_MS);

    return () => clearTimeout(timer);
  }, [term, params.companyName, setParams]);

  // Sincronización inversa: si la URL cambia desde fuera (botón "atrás" del
  // navegador, o un enlace pegado), el input tiene que reflejarlo. Cuando el cambio
  // viene del propio input, este efecto asigna el mismo valor y no hace nada — no
  // hay bucle.
  useEffect(() => {
    setTerm(params.companyName);
  }, [params.companyName]);

  const total = page?.total ?? 0;
  const totalPages = Math.max(1, Math.ceil(total / params.pageSize));
  const currentPage = Math.min(params.page, totalPages);
  const networkDown = error instanceof NetworkError;

  // ── Página fuera de rango ─────────────────────────────────
  //
  // 🇪🇸 NOTA: clampear solo el CONTADOR dejaría la vista mintiendo. Con `?page=99`
  // sobre 91 clientes la petición se hace con 99 —y vuelve vacía—, así que la tabla
  // diría "No hay clientes" mientras el paginador muestra "Página 10 de 10": dos
  // afirmaciones incompatibles sobre el mismo estado, y la de abajo es la falsa.
  //
  // Se corrige la URL, que es la fuente de verdad, y la recarga que provoca trae la
  // última página real. No hace falta teclear una URL a mano para llegar aquí: basta
  // con borrar clientes hasta que la página en la que estabas deje de existir.
  useEffect(() => {
    if (!page || params.page <= totalPages) return;
    setParams({ page: totalPages });
  }, [page, params.page, totalPages, setParams]);

  return (
    <main className="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 lg:py-10">
      {/* 🇪🇸 NOTA: la cabecera se centra, pero SOLO la cabecera. La barra de
          herramientas y la tabla siguen alineadas a los bordes porque son controles
          que se usan, no presentación que se lee: el ojo vuelve siempre al mismo
          sitio a buscar el buscador. `max-w-2xl` evita que el subtítulo se estire
          hasta los 1152 px del contenedor, donde la línea sería tan larga que
          costaría encontrar el principio de la siguiente. */}
      <header className="mb-8 text-center">
        <h1 className="text-xl font-semibold tracking-tight">
          Northwind · Customer Management
        </h1>
        <p className="mx-auto mt-2 max-w-2xl text-sm text-muted-foreground">
          Aplicación full-stack de gestión de clientes sobre la base de datos
          Northwind: API REST en Rust con Rocket y panel administrativo en
          Next.js.
        </p>
      </header>

      {/* ── Barra superior ───────────────────────────────────────────── */}
      <div className="mb-3 flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="relative sm:w-80">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground/70" />
          <Input
            value={term}
            onChange={(event) => setTerm(event.target.value)}
            placeholder="Buscar por nombre de empresa…"
            className="pl-8"
            aria-label="Buscar por nombre de empresa"
          />
        </div>

        <Button onClick={() => setCreating(true)}>
          <Plus />
          Nuevo cliente
        </Button>
      </div>

      {/* ── Tabla, o el fallo de red ─────────────────────────────────── */}
      <div className="rounded-lg border border-border bg-card">
        {error ? (
          <div className="p-4">
            {/* 🇪🇸 NOTA (el mensaje distingue los dos fallos): un `TypeError` de fetch
                no trae status ni cuerpo — puede ser el backend apagado, la red caída
                o unas cabeceras CORS ausentes. Decir "no se encontró" ahí sería
                mentira, y mandaría al usuario a buscar clientes cuando lo que tiene
                que hacer es arrancar un servidor. */}
            <Notice
              variant="error"
              title={
                networkDown
                  ? "El servidor no responde"
                  : "No se pudo cargar la lista"
              }
            >
              {error.message}
            </Notice>
            <Button
              variant="outline"
              onClick={reload}
              className="mt-3"
              size="sm"
            >
              Reintentar
            </Button>
          </div>
        ) : (
          <CustomersTable
            customers={page?.data ?? []}
            loading={loading}
            // Tantos esqueletos como filas va a haber: la tabla no cambia de
            // altura al llegar los datos y no se mueve nada bajo el cursor.
            skeletonRows={params.pageSize}
            sortBy={params.sortBy}
            sortDir={params.sortDir}
            onSort={toggleSort}
            onEdit={(customer) => setEditing(customer.customerId)}
            onDelete={setDeleting}
            emptyMessage={
              params.companyName
                ? `No hay clientes cuyo nombre contenga «${params.companyName}».`
                : "No hay clientes."
            }
          />
        )}
      </div>

      {/* ── Paginación ───────────────────────────────────────────────── */}
      <div className="mt-3 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <p className="tabular text-sm text-muted-foreground">
          {loading && !page ? (
            <span className="opacity-0">—</span>
          ) : (
            <>
              {total} {total === 1 ? "cliente" : "clientes"}
              {params.companyName ? " (filtrados)" : ""}
            </>
          )}
        </p>

        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">Por página</span>
            <Select
              value={String(params.pageSize)}
              onValueChange={(value) =>
                // Cambiar el tamaño de página también vuelve a la 1: la página 7 de
                // 10 en 10 no tiene equivalente evidente al pasar a 50 en 50.
                setParams({ pageSize: Number(value), page: 1 })
              }
            >
              <SelectTrigger className="w-[4.5rem]" aria-label="Clientes por página">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {PAGE_SIZES.map((size) => (
                  <SelectItem key={size} value={String(size)}>
                    {size}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="flex items-center gap-1">
            <span className="tabular mr-1 text-sm text-muted-foreground">
              Página {currentPage} de {totalPages}
            </span>
            <Button
              variant="outline"
              size="icon"
              onClick={() => setParams({ page: currentPage - 1 })}
              disabled={currentPage <= 1 || loading}
              aria-label="Página anterior"
            >
              <ChevronLeft />
            </Button>
            <Button
              variant="outline"
              size="icon"
              onClick={() => setParams({ page: currentPage + 1 })}
              disabled={currentPage >= totalPages || loading}
              aria-label="Página siguiente"
            >
              <ChevronRight />
            </Button>
          </div>
        </div>
      </div>

      {/* ── Diálogos ─────────────────────────────────────────────────── */}
      <CreateCustomerDialog
        open={creating}
        onOpenChange={setCreating}
        onCreated={reload}
      />
      <EditCustomerDialog
        customerId={editing}
        onOpenChange={(open) => !open && setEditing(null)}
        onUpdated={reload}
      />
      <DeleteCustomerDialog
        customer={deleting}
        onOpenChange={(open) => !open && setDeleting(null)}
        onDeleted={reload}
      />
    </main>
  );
}
