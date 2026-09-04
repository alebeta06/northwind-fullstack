"use client";

import { ChevronDown, ChevronUp, Pencil, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { Customer, SortableColumn, SortDirection } from "@/lib/api";
import { cn } from "@/lib/utils";

interface Column {
  key: keyof Customer;
  label: string;
  sortable: boolean;
  /** Se oculta en pantallas estrechas para que quepan las columnas que importan. */
  hideOnMobile?: boolean;
}

/**
 * 🇪🇸 NOTA (qué se oculta en móvil y por qué): en una pantalla de 375 px no caben
 * cinco columnas sin que todo se convierta en texto cortado. Se sacrifican
 * "Contact name" y "Country", que son contexto; se quedan el id, la empresa y la
 * ciudad, que son con lo que se identifica una fila. El dato completo sigue a un
 * clic, en el diálogo de edición — se oculta información secundaria, no se pierde.
 */
const COLUMNS: Column[] = [
  { key: "customerId", label: "Customer ID", sortable: true },
  { key: "companyName", label: "Company name", sortable: true },
  { key: "contactName", label: "Contact name", sortable: false, hideOnMobile: true },
  { key: "city", label: "City", sortable: true },
  { key: "country", label: "Country", sortable: true, hideOnMobile: true },
];

/**
 * Celda para un valor que puede ser NULL.
 *
 * 🇪🇸 NOTA: un guion atenuado y no una celda vacía. Una celda en blanco es
 * ambigua — ¿no hay dato, o la tabla se ha roto al pintar? —, y además rompe el
 * ritmo visual de la fila. El guion dice "aquí no hay nada" de forma explícita, y
 * al ir atenuado no compite con los datos reales.
 */
function Value({ children }: { children: string | null }) {
  if (children === null || children === "") {
    return (
      <span aria-label="sin dato" className="text-muted-foreground/50">
        —
      </span>
    );
  }
  return <>{children}</>;
}

export function CustomersTable({
  customers,
  loading,
  skeletonRows,
  sortBy,
  sortDir,
  onSort,
  onEdit,
  onDelete,
  emptyMessage,
}: {
  customers: Customer[];
  loading: boolean;
  skeletonRows: number;
  sortBy: SortableColumn;
  sortDir: SortDirection;
  onSort: (column: SortableColumn) => void;
  onEdit: (customer: Customer) => void;
  onDelete: (customer: Customer) => void;
  emptyMessage: string;
}) {
  return (
    <Table>
      <TableHeader>
        <TableRow className="hover:bg-transparent">
          {COLUMNS.map((column) => {
            const active = column.sortable && sortBy === column.key;
            const Chevron = sortDir === "asc" ? ChevronUp : ChevronDown;

            return (
              <TableHead
                key={column.key}
                className={cn(column.hideOnMobile && "hidden md:table-cell")}
                aria-sort={
                  active
                    ? sortDir === "asc"
                      ? "ascending"
                      : "descending"
                    : undefined
                }
              >
                {column.sortable ? (
                  <button
                    type="button"
                    onClick={() => onSort(column.key as SortableColumn)}
                    className="-mx-1 flex items-center gap-1 rounded px-1 py-1 transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    {column.label}
                    {/* 🇪🇸 NOTA: el chevron SOLO en la columna activa. Poner una
                        flecha tenue en cada cabecera "por descubribilidad" añade
                        cinco elementos que compiten entre sí y hace más difícil ver
                        cuál manda. El estado actual debe leerse de un vistazo. */}
                    {active ? (
                      <Chevron className="size-3.5 text-foreground" />
                    ) : null}
                  </button>
                ) : (
                  column.label
                )}
              </TableHead>
            );
          })}
          <TableHead className="w-20 text-right">
            <span className="sr-only">Acciones</span>
          </TableHead>
        </TableRow>
      </TableHeader>

      <TableBody>
        {loading ? (
          Array.from({ length: skeletonRows }).map((_, index) => (
            <TableRow key={index} className="hover:bg-transparent">
              {COLUMNS.map((column) => (
                <TableCell
                  key={column.key}
                  className={cn(column.hideOnMobile && "hidden md:table-cell")}
                >
                  <Skeleton className="h-4 w-[70%]" />
                </TableCell>
              ))}
              <TableCell>
                <Skeleton className="ml-auto h-4 w-12" />
              </TableCell>
            </TableRow>
          ))
        ) : customers.length === 0 ? (
          <TableRow className="hover:bg-transparent">
            {/* 🇪🇸 NOTA: el mensaje va DENTRO de la tabla, en una celda que ocupa
                todo el ancho. Sacarlo fuera haría desaparecer las cabeceras, y con
                ellas el contexto de qué se estaba mirando y cómo estaba ordenado. */}
            <TableCell
              colSpan={COLUMNS.length + 1}
              className="h-32 text-center text-sm text-muted-foreground"
            >
              {emptyMessage}
            </TableCell>
          </TableRow>
        ) : (
          customers.map((customer) => (
            <TableRow key={customer.customerId}>
              <TableCell className="tabular font-medium">
                {customer.customerId}
              </TableCell>
              <TableCell className="max-w-[22rem] truncate">
                {customer.companyName}
              </TableCell>
              <TableCell className="hidden md:table-cell">
                <Value>{customer.contactName}</Value>
              </TableCell>
              <TableCell>
                <Value>{customer.city}</Value>
              </TableCell>
              <TableCell className="hidden md:table-cell">
                <Value>{customer.country}</Value>
              </TableCell>
              <TableCell className="text-right">
                <div className="flex justify-end gap-0.5">
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => onEdit(customer)}
                    aria-label={`Editar ${customer.companyName}`}
                  >
                    <Pencil />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => onDelete(customer)}
                    aria-label={`Borrar ${customer.companyName}`}
                    className="text-muted-foreground hover:text-destructive"
                  >
                    <Trash2 />
                  </Button>
                </div>
              </TableCell>
            </TableRow>
          ))
        )}
      </TableBody>
    </Table>
  );
}
