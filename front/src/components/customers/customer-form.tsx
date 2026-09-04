"use client";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import type { Customer, CustomerFields } from "@/lib/api";

/**
 * Los diez campos editables, como texto plano.
 *
 * 🇪🇸 NOTA (por qué el formulario usa `string` y no `string | null`): un `<input>`
 * solo sabe de cadenas. Modelar aquí el `null` obligaría a convertir en cada
 * pulsación y a decidir si un campo que el usuario acaba de vaciar es `""` o `null`
 * mientras escribe. Se trabaja con cadenas y la conversión ocurre en UN solo sitio,
 * `toFields()`, justo antes de enviar — que es exactamente donde el backend hace lo
 * mismo con su `optional_text()`.
 */
export type FormValues = Record<keyof CustomerFields, string>;

export const EMPTY_FORM: FormValues = {
  companyName: "",
  contactName: "",
  contactTitle: "",
  address: "",
  city: "",
  region: "",
  postalCode: "",
  country: "",
  phone: "",
  fax: "",
};

export function formFromCustomer(customer: Customer): FormValues {
  return {
    companyName: customer.companyName,
    contactName: customer.contactName ?? "",
    contactTitle: customer.contactTitle ?? "",
    address: customer.address ?? "",
    city: customer.city ?? "",
    region: customer.region ?? "",
    postalCode: customer.postalCode ?? "",
    country: customer.country ?? "",
    phone: customer.phone ?? "",
    fax: customer.fax ?? "",
  };
}

/**
 * Convierte el formulario al cuerpo que espera la API.
 *
 * 🇪🇸 NOTA: un campo vacío se manda como `null`, no como `""`. Es la misma decisión
 * que toma el backend en `optional_text()`, y hacerla también aquí evita mandar
 * ruido: "sin dato" y "dato vacío" son cosas distintas, y mezclarlas en la columna
 * hace que `WHERE Region IS NULL` deje de encontrar la mitad de las filas.
 */
export function toFields(values: FormValues): CustomerFields {
  const clean = (value: string) => {
    const trimmed = value.trim();
    return trimmed === "" ? null : trimmed;
  };

  return {
    companyName: values.companyName.trim(),
    contactName: clean(values.contactName),
    contactTitle: clean(values.contactTitle),
    address: clean(values.address),
    city: clean(values.city),
    region: clean(values.region),
    postalCode: clean(values.postalCode),
    country: clean(values.country),
    phone: clean(values.phone),
    fax: clean(values.fax),
  };
}

const FIELDS: Array<{
  name: keyof FormValues;
  label: string;
  wide?: boolean;
  autoComplete?: string;
}> = [
  { name: "companyName", label: "Company name *", wide: true },
  { name: "contactName", label: "Contact name" },
  { name: "contactTitle", label: "Contact title" },
  { name: "address", label: "Address", wide: true },
  { name: "city", label: "City" },
  { name: "region", label: "Region" },
  { name: "postalCode", label: "Postal code" },
  { name: "country", label: "Country" },
  { name: "phone", label: "Phone" },
  { name: "fax", label: "Fax" },
];

export function CustomerFormFields({
  values,
  onChange,
  disabled,
  loading,
}: {
  values: FormValues;
  onChange: (name: keyof FormValues, value: string) => void;
  disabled?: boolean;
  loading?: boolean;
}) {
  if (loading) {
    // Los mismos huecos que ocupará el formulario: el diálogo no cambia de tamaño
    // cuando llegan los datos.
    return (
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {FIELDS.map((field) => (
          <div
            key={field.name}
            className={field.wide ? "sm:col-span-2" : undefined}
          >
            <Skeleton className="mb-1.5 h-3 w-24" />
            <Skeleton className="h-9 w-full" />
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
      {FIELDS.map((field) => (
        <div
          key={field.name}
          className={field.wide ? "sm:col-span-2" : undefined}
        >
          <Label htmlFor={field.name} className="mb-1.5 block">
            {field.label}
          </Label>
          <Input
            id={field.name}
            name={field.name}
            value={values[field.name]}
            onChange={(event) => onChange(field.name, event.target.value)}
            disabled={disabled}
            autoComplete="off"
          />
        </div>
      ))}
    </div>
  );
}
