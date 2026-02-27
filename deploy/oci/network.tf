resource "oci_core_vcn" "zerooraclaw" {
  compartment_id = var.compartment_ocid
  display_name   = "zerooraclaw-vcn"
  cidr_blocks    = [var.vcn_cidr]
  dns_label      = "zerovcn"
}

resource "oci_core_internet_gateway" "zerooraclaw" {
  compartment_id = var.compartment_ocid
  vcn_id         = oci_core_vcn.zerooraclaw.id
  display_name   = "zerooraclaw-igw"
  enabled        = true
}

resource "oci_core_route_table" "zerooraclaw" {
  compartment_id = var.compartment_ocid
  vcn_id         = oci_core_vcn.zerooraclaw.id
  display_name   = "zerooraclaw-rt"

  route_rules {
    destination       = "0.0.0.0/0"
    network_entity_id = oci_core_internet_gateway.zerooraclaw.id
  }
}

resource "oci_core_security_list" "zerooraclaw" {
  compartment_id = var.compartment_ocid
  vcn_id         = oci_core_vcn.zerooraclaw.id
  display_name   = "zerooraclaw-sl"

  # Allow all egress
  egress_security_rules {
    destination = "0.0.0.0/0"
    protocol    = "all"
    stateless   = false
  }

  # SSH
  ingress_security_rules {
    source    = "0.0.0.0/0"
    protocol  = "6"
    stateless = false
    tcp_options {
      min = 22
      max = 22
    }
  }

  # Gateway API
  ingress_security_rules {
    source    = "0.0.0.0/0"
    protocol  = "6"
    stateless = false
    tcp_options {
      min = 42617
      max = 42617
    }
  }

  # ICMP
  ingress_security_rules {
    source    = "0.0.0.0/0"
    protocol  = "1"
    stateless = false
    icmp_options {
      type = 3
      code = 4
    }
  }
}

resource "oci_core_subnet" "zerooraclaw" {
  compartment_id             = var.compartment_ocid
  vcn_id                     = oci_core_vcn.zerooraclaw.id
  display_name               = "zerooraclaw-subnet"
  cidr_block                 = cidrsubnet(var.vcn_cidr, 8, 1)
  dns_label                  = "zerosub"
  route_table_id             = oci_core_route_table.zerooraclaw.id
  security_list_ids          = [oci_core_security_list.zerooraclaw.id]
  prohibit_public_ip_on_vnic = false
}
